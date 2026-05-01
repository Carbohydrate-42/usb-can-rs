// ============================================
// zencan 适配层：为 BusManager 提供兼容接口
// ============================================

use tokio::sync::mpsc;
use zencan_common::{CanId, CanMessage, traits::{AsyncCanReceiver, AsyncCanSender, CanSendError}};

use crate::{CanUsbConfig, ClientError, Frame, client_with_split::CanUsbSender, split};

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
    undelivered: Option<CanMessage>,
}

impl ZenCanSendError {
    pub fn new(msg: String, undelivered: Option<CanMessage>) -> Self {
        Self { msg, undelivered }
    }
}

impl core::fmt::Display for ZenCanSendError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.msg)
    }
}

impl CanSendError for ZenCanSendError {
    fn into_can_message(self) -> CanMessage {
        self.undelivered.unwrap_or_else(|| {
            // 如果没有保存消息，创建一个空的作为占位
            CanMessage::new(CanId::Std(0), &[])
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

// 修改 ZenCanSender 的 send 实现以支持错误时返回消息
impl AsyncCanSender for ZenCanSender {
    type Error = ZenCanSendError;

    async fn send(&mut self, msg: CanMessage) -> Result<(), Self::Error> {
        let frame = Frame::from_message(msg);
        
        self.inner.send(frame).await.map_err(|e| {
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

    fn try_recv(&mut self) -> Option<CanMessage> {
        self.inner.try_recv().ok()
    }

    async fn recv(&mut self) -> Result<CanMessage, Self::Error> {
        self.inner.recv().await.ok_or_else(|| {
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
    config: CanUsbConfig,
) -> Result<(ZenCanSender, ZenCanReceiver), ClientError> {
    let (tx, rx) = split(config).await?;
    Ok((ZenCanSender::new(tx), ZenCanReceiver::new(rx)))
}