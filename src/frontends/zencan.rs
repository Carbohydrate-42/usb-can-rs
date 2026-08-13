//! zencan frontend: provides `BusManager`-compatible sender/receiver.
//!
//! Converts between this crate's `embedded-can` based [`CanMessage`] and
//! `zencan-common`'s message types, and implements zencan's
//! `AsyncCanSender` / `AsyncCanReceiver` traits.

use tokio::sync::mpsc;
use zencan_common::traits::{AsyncCanReceiver, AsyncCanSender, CanSendError};
use zencan_common::{CanId, CanMessage as ZenCanMessage};

use crate::message::CanMessage;
use crate::backends::tokio_serial::{split, CanUsbSender, ClientError, TokioSerialConfig};
use embedded_can::{ExtendedId, Id, StandardId};

/// Convert a zencan message into this crate's `embedded-can` based message.
fn from_zencan(msg: &ZenCanMessage) -> Option<CanMessage> {
    let id = match msg.id {
        CanId::Std(raw) => Id::Standard(StandardId::new(raw)?),
        CanId::Extended(raw) => Id::Extended(ExtendedId::new(raw)?),
    };
    if msg.rtr {
        CanMessage::new_rtr(id, msg.dlc)
    } else {
        CanMessage::new(id, msg.data())
    }
}

/// Convert our message into a zencan message.
fn to_zencan(msg: &CanMessage) -> ZenCanMessage {
    let id = match msg.id() {
        Id::Standard(std) => CanId::Std(std.as_raw()),
        Id::Extended(ext) => CanId::Extended(ext.as_raw()),
    };
    if msg.is_rtr() {
        ZenCanMessage::new_rtr(id)
    } else {
        ZenCanMessage::new(id, msg.data())
    }
}

/// Adapter implementing zencan's `AsyncCanSender`
pub struct ZenCanSender {
    inner: CanUsbSender,
}

impl ZenCanSender {
    pub fn new(sender: CanUsbSender) -> Self {
        Self { inner: sender }
    }
}

/// Send error adapter
#[derive(Debug)]
pub struct ZenCanSendError {
    msg: String,
    undelivered: Option<ZenCanMessage>,
}

impl ZenCanSendError {
    pub fn new(msg: String, undelivered: Option<ZenCanMessage>) -> Self {
        Self { msg, undelivered }
    }
}

impl core::fmt::Display for ZenCanSendError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.msg)
    }
}

impl std::error::Error for ZenCanSendError {}

impl CanSendError for ZenCanSendError {
    fn into_can_message(self) -> ZenCanMessage {
        self.undelivered.unwrap_or_else(|| {
            // If the original message was not kept, create an empty placeholder
            ZenCanMessage::new(CanId::Std(0), &[])
        })
    }

    fn message(&self) -> String {
        self.msg.clone()
    }
}

impl From<ClientError> for ZenCanSendError {
    fn from(e: ClientError) -> Self {
        // ClientError does not carry the original message, so undelivered is None.
        // To keep the message, construct the error manually in send().
        ZenCanSendError::new(e.to_string(), None)
    }
}

impl AsyncCanSender for ZenCanSender {
    type Error = ZenCanSendError;

    async fn send(&mut self, msg: ZenCanMessage) -> Result<(), Self::Error> {
        let converted = from_zencan(&msg)
            .ok_or_else(|| ZenCanSendError::new("Invalid CAN message".to_string(), Some(msg.clone())))?;

        self.inner.send(converted.into()).await.map_err(|e| {
            // Send failed: return the error together with the original message
            ZenCanSendError::new(e.to_string(), Some(msg))
        })
    }
}

/// Adapter implementing zencan's `AsyncCanReceiver`
pub struct ZenCanReceiver {
    inner: mpsc::Receiver<CanMessage>,
}

impl ZenCanReceiver {
    pub fn new(receiver: mpsc::Receiver<CanMessage>) -> Self {
        Self { inner: receiver }
    }
}

#[derive(Debug)]
pub struct ZenCanRecvError(pub String);

impl core::fmt::Display for ZenCanRecvError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ZenCanRecvError {}

impl AsyncCanReceiver for ZenCanReceiver {
    type Error = ZenCanRecvError;

    fn try_recv(&mut self) -> Option<ZenCanMessage> {
        self.inner.try_recv().ok().map(|m| to_zencan(&m))
    }

    async fn recv(&mut self) -> Result<ZenCanMessage, Self::Error> {
        self.inner.recv().await.map(|m| to_zencan(&m)).ok_or_else(|| {
            ZenCanRecvError("Channel closed".to_string())
        })
    }

    fn flush(&mut self) {
        while self.try_recv().is_some() {}
    }
}

/// Split function dedicated to zencan's BusManager
///
/// Returns a sender and receiver that can be passed directly to `BusManager::new`.
pub async fn split_for_zencan(
    config: TokioSerialConfig,
) -> Result<(ZenCanSender, ZenCanReceiver), ClientError> {
    let (tx, rx) = split(config).await?;
    Ok((ZenCanSender::new(tx), ZenCanReceiver::new(rx)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standard_message_roundtrip() {
        let zen = ZenCanMessage::new(CanId::Std(0x123), &[0x11, 0x22, 0x33]);
        let ours = from_zencan(&zen).unwrap();

        assert_eq!(ours.id(), Id::Standard(StandardId::new(0x123).unwrap()));
        assert_eq!(ours.data(), &[0x11, 0x22, 0x33]);
        assert!(!ours.is_rtr());

        let back = to_zencan(&ours);
        assert_eq!(back.id, CanId::Std(0x123));
        assert_eq!(back.data(), &[0x11, 0x22, 0x33]);
        assert!(!back.rtr);
    }

    #[test]
    fn test_extended_message_roundtrip() {
        let zen = ZenCanMessage::new(CanId::Extended(0x1ABCDE), &[0xAA]);
        let ours = from_zencan(&zen).unwrap();

        assert_eq!(ours.id(), Id::Extended(ExtendedId::new(0x1ABCDE).unwrap()));
        assert_eq!(ours.data(), &[0xAA]);

        let back = to_zencan(&ours);
        assert_eq!(back.id, CanId::Extended(0x1ABCDE));
        assert_eq!(back.data(), &[0xAA]);
    }

    #[test]
    fn test_rtr_message_roundtrip() {
        let zen = ZenCanMessage::new_rtr(CanId::Std(0x100));
        let ours = from_zencan(&zen).unwrap();

        assert!(ours.is_rtr());
        assert_eq!(ours.data(), &[]);

        let back = to_zencan(&ours);
        assert!(back.rtr);
        assert_eq!(back.id, CanId::Std(0x100));
    }

    #[test]
    fn test_invalid_standard_id_rejected() {
        // 0x800 does not fit in 11 bits
        let zen = ZenCanMessage::new(CanId::Std(0x800), &[]);
        assert!(from_zencan(&zen).is_none());
    }

    #[test]
    fn test_empty_data_roundtrip() {
        let zen = ZenCanMessage::new(CanId::Std(0x1), &[]);
        let ours = from_zencan(&zen).unwrap();
        assert_eq!(ours.dlc(), 0);

        let back = to_zencan(&ours);
        assert_eq!(back.data(), &[]);
    }
}
