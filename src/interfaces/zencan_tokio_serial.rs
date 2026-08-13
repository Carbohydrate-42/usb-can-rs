//! zencan adapter over the tokio-serial transport: provides
//! `BusManager`-compatible sender/receiver.
//!
//! Wraps the split mode of [`crate::adapters::tokio_serial`], converting
//! between `embedded-can` based frames and `zencan-common`'s message types,
//! and implements zencan's `AsyncCanSender` / `AsyncCanReceiver` traits.

use tokio::sync::mpsc;
use zencan_common::traits::{AsyncCanReceiver, AsyncCanSender, CanSendError};
use zencan_common::{CanId, CanMessage as ZenCanMessage};

use crate::adapters::tokio_serial::{CanUsbSender, ClientError};
use crate::message::CanMessage;
use crate::protocol::Protocol;
use embedded_can::{ExtendedId, Frame, Id, StandardId};

// ============================================
// Sender
// ============================================

/// Adapter implementing zencan's `AsyncCanSender`
pub struct ZenCanSender {
    inner: CanUsbSender,
}

impl ZenCanSender {
    /// Split function dedicated to zencan's BusManager
    ///
    /// Returns a sender and receiver that can be passed directly to `BusManager::new`.
    ///
    /// # Arguments
    /// * `serial` - serial port opened by the caller (e.g. via `open_native_async`)
    /// * `protocol` - wire protocol implementation (e.g. [`crate::protocol::wareshare_usb_can_a::WaveshareUsbCanA`])
    /// * `config` - protocol-specific configuration
    pub async fn split<P>(
        serial: tokio_serial::SerialStream,
        protocol: P,
        config: &P::Config,
    ) -> Result<(Self, ZenCanReceiver), ClientError>
    where
        P: Protocol + Send + Sync + 'static,
        P::Config: Send + Sync + 'static,
    {
        let (tx, rx) = CanUsbSender::split(serial, protocol, config).await?;
        Ok((Self { inner: tx }, ZenCanReceiver { inner: rx }))
    }

    /// Convert a zencan message into an `embedded-can` based message.
    ///
    /// Returns `None` if the ID or DLC is out of range.
    pub fn from_zencan(msg: &ZenCanMessage) -> Option<CanMessage> {
        let id = match msg.id {
            CanId::Std(raw) => Id::Standard(StandardId::new(raw)?),
            CanId::Extended(raw) => Id::Extended(ExtendedId::new(raw)?),
        };
        if msg.rtr {
            CanMessage::new_remote(id, msg.dlc as usize)
        } else {
            CanMessage::new(id, msg.data())
        }
    }
}

impl AsyncCanSender for ZenCanSender {
    type Error = ZenCanSendError;

    async fn send(&mut self, msg: ZenCanMessage) -> Result<(), Self::Error> {
        let converted = Self::from_zencan(&msg)
            .ok_or_else(|| ZenCanSendError::new("Invalid CAN message".to_string(), Some(msg)))?;

        self.inner.send(&converted).await.map_err(|e| {
            // Send failed: return the error together with the original message
            ZenCanSendError::new(e.to_string(), Some(msg))
        })
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

// ============================================
// Receiver
// ============================================

/// Adapter implementing zencan's `AsyncCanReceiver`
pub struct ZenCanReceiver {
    inner: mpsc::Receiver<CanMessage>,
}

impl ZenCanReceiver {
    /// Convert an `embedded-can` frame into a zencan message.
    pub fn to_zencan(msg: &impl Frame) -> ZenCanMessage {
        let id = match msg.id() {
            Id::Standard(std) => CanId::Std(std.as_raw()),
            Id::Extended(ext) => CanId::Extended(ext.as_raw()),
        };
        if msg.is_remote_frame() {
            ZenCanMessage::new_rtr(id)
        } else {
            ZenCanMessage::new(id, msg.data())
        }
    }
}

impl AsyncCanReceiver for ZenCanReceiver {
    type Error = ZenCanRecvError;

    fn try_recv(&mut self) -> Option<ZenCanMessage> {
        self.inner.try_recv().ok().map(|m| Self::to_zencan(&m))
    }

    async fn recv(&mut self) -> Result<ZenCanMessage, Self::Error> {
        self.inner
            .recv()
            .await
            .map(|m| Self::to_zencan(&m))
            .ok_or_else(|| ZenCanRecvError("Channel closed".to_string()))
    }

    fn flush(&mut self) {
        while self.try_recv().is_some() {}
    }
}

/// Receive error adapter
#[derive(Debug)]
pub struct ZenCanRecvError(pub String);

impl core::fmt::Display for ZenCanRecvError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ZenCanRecvError {}
