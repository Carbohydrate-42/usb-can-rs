//! zencan adaptor: provides `BusManager`-compatible sender/receiver.
//!
//! Converts between this crate's `embedded-can` based [`CanMessage`] and
//! `zencan-common`'s message types, and implements zencan's
//! `AsyncCanSender` / `AsyncCanReceiver` traits.

use tokio::sync::mpsc;
use zencan_common::traits::{AsyncCanReceiver, AsyncCanSender, CanSendError};
use zencan_common::{CanId, CanMessage as ZenCanMessage};

use crate::message::CanMessage;
use crate::tokio_serial::{split, CanUsbSender, ClientError, TokioSerialConfig};
use embedded_can::{ExtendedId, Id, StandardId};

/// Convert a zencan message into our `embedded-can` based message.
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

/// 适配 zencan 的 AsyncCanSender
pub struct ZenCanSender {
    inner: CanUsbSender,
}

impl ZenCanSender {
    pub fn new(sender: CanUsbSender) -> Self {
        Self { inner: sender }
    }
}

/// 发送错误适配
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

impl CanSendError for ZenCanSendError {
    fn into_can_message(self) -> ZenCanMessage {
        self.undelivered.unwrap_or_else(|| {
            // 如果没有保存消息，创建一个空的作为占位
            ZenCanMessage::new(CanId::Std(0), &[])
        })
    }

    fn message(&self) -> String {
        self.msg.clone()
    }
}

impl From<ClientError> for ZenCanSendError {
    fn from(e: ClientError) -> Self {
        // ClientError 不包含原始消息，所以 undelivered 为 None
        // 如果需要保留消息，需要在 send 方法里手动构造
        ZenCanSendError::new(e.to_string(), None)
    }
}

impl AsyncCanSender for ZenCanSender {
    type Error = ZenCanSendError;

    async fn send(&mut self, msg: ZenCanMessage) -> Result<(), Self::Error> {
        let converted = from_zencan(&msg)
            .ok_or_else(|| ZenCanSendError::new("Invalid CAN message".to_string(), Some(msg.clone())))?;

        self.inner.send(converted.into()).await.map_err(|e| {
            // 发送失败，返回错误并附带原始消息
            ZenCanSendError::new(e.to_string(), Some(msg))
        })
    }
}

/// 适配 zencan 的 AsyncCanReceiver
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

/// 专为 zencan BusManager 创建的 split 函数
///
/// 返回可以直接传给 BusManager::new 的 sender 和 receiver
pub async fn split_for_zencan(
    config: TokioSerialConfig,
) -> Result<(ZenCanSender, ZenCanReceiver), ClientError> {
    let (tx, rx) = split(config).await?;
    Ok((ZenCanSender::new(tx), ZenCanReceiver::new(rx)))
}
