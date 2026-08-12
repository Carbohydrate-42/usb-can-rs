//! std adapter: tokio-serial transport.
//!
//! Async CAN USB client over a serial port, supporting both a channel-style
//! split mode and an exclusive client mode.

use crate::frame::Frame;
#[allow(unused_imports)]
use crate::logging::{error, info, trace, Fmt, Hex};
use crate::message::CanMessage;
use crate::protocol::{build_data_frame, build_settings_frame, parse_frames};
use crate::types::CanUsbConfig;
use embedded_can::{ExtendedId, Id, StandardId};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, Mutex};
use tokio::time::{sleep, timeout};
use tokio_serial::SerialPortBuilderExt;

/// Serial transport configuration (device path + baudrate + CAN settings).
#[derive(Debug, Clone)]
pub struct TokioSerialConfig {
    /// Serial device path (e.g., "/dev/ttyUSB0" or "COM4")
    pub device: String,
    /// Serial baudrate (default: 2000000)
    pub baudrate: u32,
    /// CAN-side configuration
    pub can: CanUsbConfig,
}

impl TokioSerialConfig {
    /// Create a config for the given device with default CAN settings.
    pub fn new(device: impl Into<String>) -> Self {
        Self {
            device: device.into(),
            baudrate: 2_000_000,
            can: CanUsbConfig::default(),
        }
    }
}

/// Client errors
#[derive(Debug)]
pub enum ClientError {
    /// Serial port error
    Serial(tokio_serial::Error),
    /// IO error
    Io(std::io::Error),
    /// Frame too large
    FrameTooLarge,
    /// Write timeout
    WriteTimeout,
    /// Read timeout
    ReadTimeout,
    /// Message channel send error
    SendError(mpsc::error::SendError<CanMessage>),
    /// Frame channel send error
    FrameSendError(mpsc::error::SendError<Frame>),
    /// Channel closed
    ChannelClosed,
}

impl core::fmt::Display for ClientError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ClientError::Serial(e) => write!(f, "Serial port error: {}", e),
            ClientError::Io(e) => write!(f, "IO error: {}", e),
            ClientError::FrameTooLarge => write!(f, "Frame too large"),
            ClientError::WriteTimeout => write!(f, "Write timeout"),
            ClientError::ReadTimeout => write!(f, "Read timeout"),
            ClientError::SendError(e) => write!(f, "Send error: {}", e),
            ClientError::FrameSendError(e) => write!(f, "Frame send error: {}", e),
            ClientError::ChannelClosed => write!(f, "Channel closed"),
        }
    }
}

impl std::error::Error for ClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ClientError::Serial(e) => Some(e),
            ClientError::Io(e) => Some(e),
            ClientError::SendError(e) => Some(e),
            ClientError::FrameSendError(e) => Some(e),
            _ => None,
        }
    }
}

impl From<tokio_serial::Error> for ClientError {
    fn from(e: tokio_serial::Error) -> Self {
        ClientError::Serial(e)
    }
}

impl From<std::io::Error> for ClientError {
    fn from(e: std::io::Error) -> Self {
        ClientError::Io(e)
    }
}

impl From<mpsc::error::SendError<CanMessage>> for ClientError {
    fn from(e: mpsc::error::SendError<CanMessage>) -> Self {
        ClientError::SendError(e)
    }
}

impl From<mpsc::error::SendError<Frame>> for ClientError {
    fn from(e: mpsc::error::SendError<Frame>) -> Self {
        ClientError::FrameSendError(e)
    }
}

/// Convert a parsed wire frame into a [`CanMessage`].
fn parsed_to_message(parsed: &crate::protocol::ParsedFrame) -> Option<CanMessage> {
    let can_id = if parsed.is_extended {
        Id::Extended(ExtendedId::new(parsed.id as u32)?)
    } else {
        Id::Standard(StandardId::new(parsed.id)?)
    };
    CanMessage::new(can_id, &parsed.data)
}

// ============================================
// Split 模式：Channel 风格
// ============================================

/// 发送端 Sender - 通过 channel 发送 Frame，后台任务实际写入串口
#[derive(Clone)]
pub struct CanUsbSender {
    frame_tx: mpsc::Sender<Frame>,
}

impl CanUsbSender {
    /// 异步发送 CAN 帧（非阻塞，缓冲到 channel）
    pub async fn send(&self, frame: Frame) -> Result<(), ClientError> {
        self.frame_tx.send(frame).await.map_err(Into::into)
    }

    /// 尝试发送（非阻塞）
    pub fn try_send(&self, frame: Frame) -> Result<(), mpsc::error::TrySendError<Frame>> {
        self.frame_tx.try_send(frame)
    }

    /// 检查 channel 是否关闭
    pub fn is_closed(&self) -> bool {
        self.frame_tx.is_closed()
    }
}

/// 创建 Split 模式，返回 (sender, receiver)
///
/// sender 可以 clone，支持多生产者并发发送
/// receiver 是唯一的，用于接收 CAN 消息
pub async fn split(
    config: TokioSerialConfig,
) -> Result<(CanUsbSender, mpsc::Receiver<CanMessage>), ClientError> {
    info!("Creating Split mode: {} @ {} baud", Fmt(&config.device), config.baudrate);

    // 打开串口
    let serial = tokio_serial::new(&config.device, config.baudrate)
        .data_bits(tokio_serial::DataBits::Eight)
        .stop_bits(tokio_serial::StopBits::Two)
        .parity(tokio_serial::Parity::None)
        .open_native_async()?;

    let serial = Arc::new(Mutex::new(serial));

    // 创建 channel：发送 Frame 的 channel（用户 -> 后台写入任务）
    let (frame_tx, frame_rx) = mpsc::channel::<Frame>(100);
    // 接收 CanMessage 的 channel（后台读取任务 -> 用户）
    let (msg_tx, msg_rx) = mpsc::channel::<CanMessage>(100);

    // 发送初始设置
    {
        let settings = build_settings_frame(
            config.can.can_speed as u8,
            config.can.can_mode as u8,
            config.can.frame_type as u8,
            config.can.filter_id,
            config.can.mask_id,
        );
        let mut port = serial.lock().await;
        port.write_all(&settings).await?;
        port.flush().await?;
        sleep(Duration::from_millis(100)).await;
    }
    info!("Settings sent successfully");

    // 启动后台任务
    tokio::spawn(split_background_task(
        serial,
        frame_rx,
        msg_tx,
        config,
    ));

    let sender = CanUsbSender { frame_tx };

    Ok((sender, msg_rx))
}

/// Split 模式的后台任务：分离的读取和写入任务
///
/// 方案：启动两个任务，一个专门写，一个专门读
/// 避免 Mutex 竞争，利用串口全双工特性
async fn split_background_task(
    serial: Arc<Mutex<tokio_serial::SerialStream>>,
    mut frame_rx: mpsc::Receiver<Frame>,
    msg_tx: mpsc::Sender<CanMessage>,
    config: TokioSerialConfig,
) {
    // 创建内部 channel 用于两个任务间协调（如果需要）
    // 但串口是全双工的，读写可以真正并行

    let serial_read = serial.clone();
    let serial_write = serial;

    let config_read = config.clone();
    let config_write = config;

    // 读取任务
    let read_task = tokio::spawn(async move {
        let mut rx_buffer = vec![0u8; 1024];
        let mut rx_length: usize = 0;
        let mut temp = vec![0u8; 256];

        loop {
            // 获取锁读取
            let bytes_read = {
                let mut port = match timeout(Duration::from_millis(100), serial_read.lock()).await {
                    Ok(p) => p,
                    Err(_) => continue, // 获取锁超时，重试
                };

                match timeout(Duration::from_millis(50), port.read(&mut temp)).await {
                    Ok(Ok(n)) => n,
                    Ok(Err(_e)) => {
                        error!("Serial read error: {}", Fmt(&_e));
                        return;
                    }
                    Err(_) => 0, // 读超时
                }
            };

            if bytes_read == 0 {
                sleep(Duration::from_micros(100)).await;
                continue;
            }

            if config_read.can.debug_traffic {
                trace!("RX raw: {:?}", Hex(&temp[..bytes_read]));
            }

            // 复制到缓冲区
            if rx_length + bytes_read > rx_buffer.len() {
                rx_length = 0;
            }
            rx_buffer[rx_length..rx_length + bytes_read]
                .copy_from_slice(&temp[..bytes_read]);
            rx_length += bytes_read;

            // 解析帧
            let mut parsed_frames = Vec::new();
            let consumed = parse_frames(
                &rx_buffer[..rx_length],
                &mut parsed_frames,
                config_read.can.debug_traffic
            );

            if consumed > 0 {
                rx_buffer.copy_within(consumed..rx_length, 0);
                rx_length -= consumed;
            }

            // 发送消息
            for parsed in parsed_frames {
                let msg = match parsed_to_message(&parsed) {
                    Some(m) => m,
                    None => {
                        error!("Invalid parsed frame (bad id or dlc > 8), dropping");
                        continue;
                    }
                };

                if msg_tx.send(msg).await.is_err() {
                    error!("Message channel closed, read task exiting");
                    return;
                }
            }
        }
    });

    // 写入任务
    let write_task = tokio::spawn(async move {
        while let Some(frame) = frame_rx.recv().await {
            // 构建数据帧
            let data = match build_data_frame(
                frame.frame_type,
                frame.raw_id(),
                frame.data()
            ) {
                Ok(d) => d,
                Err(_) => {
                    error!("Frame too large, dropping");
                    continue;
                }
            };

            // 获取锁写入
            let mut port = match timeout(Duration::from_secs(5), serial_write.lock()).await {
                Ok(p) => p,
                Err(_) => {
                    error!("Timeout acquiring serial lock for write");
                    continue;
                }
            };

            if config_write.can.debug_traffic {
                trace!("TX: {:?}", Hex(&data));
            }

            if let Err(_e) = port.write_all(&data).await {
                error!("Serial write error: {}", Fmt(&_e));
                return;
            }
            if let Err(_e) = port.flush().await {
                error!("Serial flush error: {}", Fmt(&_e));
                return;
            }
        }

        info!("Frame channel closed, write task exiting");
    });

    // 等待任一任务结束
    tokio::select! {
        _ = read_task => error!("Read task exited"),
        _ = write_task => error!("Write task exited"),
    }

    error!("Split background task exited!");
}

// ============================================
// Client 模式：独占式读写
// ============================================

/// 独占式客户端，同时只能进行 read 或 write
pub struct CanUsbClient {
    serial: tokio_serial::SerialStream,
    config: TokioSerialConfig,
    rx_buffer: Vec<u8>,
    rx_length: usize,
    temp: Vec<u8>,
}

impl CanUsbClient {
    /// 创建新的 Client 模式（独占串口）
    pub async fn new(config: TokioSerialConfig) -> Result<Self, ClientError> {
        info!("Creating Client mode: {} @ {} baud", Fmt(&config.device), config.baudrate);

        let serial = tokio_serial::new(&config.device, config.baudrate)
            .data_bits(tokio_serial::DataBits::Eight)
            .stop_bits(tokio_serial::StopBits::Two)
            .parity(tokio_serial::Parity::None)
            .open_native_async()?;

        let mut client = Self {
            serial,
            config: config.clone(),
            rx_buffer: vec![0u8; 1024],
            rx_length: 0,
            temp: vec![0u8; 256],
        };

        client.send_settings().await?;

        Ok(client)
    }

    async fn send_settings(&mut self) -> Result<(), ClientError> {
        let frame = build_settings_frame(
            self.config.can.can_speed as u8,
            self.config.can.can_mode as u8,
            self.config.can.frame_type as u8,
            self.config.can.filter_id,
            self.config.can.mask_id,
        );
        self.serial.write_all(&frame).await?;
        self.serial.flush().await?;
        sleep(Duration::from_millis(100)).await;
        info!("Settings sent successfully");
        Ok(())
    }

    pub async fn write(&mut self, frame: &Frame) -> Result<(), ClientError> {
        let data = build_data_frame(frame.frame_type, frame.raw_id(), frame.data())
            .map_err(|_| ClientError::FrameTooLarge)?;

        if self.config.can.debug_traffic {
            trace!("TX: {:?}", Hex(&data));
        }

        self.serial.write_all(&data).await?;
        self.serial.flush().await?;
        Ok(())
    }

    pub async fn read(&mut self, timeout_duration: Duration) -> Result<CanMessage, ClientError> {
        let deadline = tokio::time::Instant::now() + timeout_duration;

        loop {
            // 先尝试解析缓冲区
            let mut parsed_frames = Vec::new();
            let consumed = parse_frames(&self.rx_buffer[..self.rx_length], &mut parsed_frames, false);

            if consumed > 0 {
                self.rx_buffer.copy_within(consumed..self.rx_length, 0);
                self.rx_length -= consumed;
            }

            if let Some(parsed) = parsed_frames.into_iter().next() {
                if let Some(msg) = parsed_to_message(&parsed) {
                    return Ok(msg);
                }
                error!("Invalid parsed frame (bad id or dlc > 8), dropping");
            }

            let now = tokio::time::Instant::now();
            if now >= deadline {
                return Err(ClientError::ReadTimeout);
            }
            let remaining = deadline - now;

            match timeout(remaining, self.serial.read(&mut self.temp)).await {
                Ok(Ok(0)) => {
                    return Err(ClientError::Io(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "Serial port closed"
                    )));
                }
                Ok(Ok(n)) => {
                    if self.config.can.debug_traffic {
                        trace!("RX raw: {:?}", Hex(&self.temp[..n]));
                    }

                    if self.rx_length + n > self.rx_buffer.len() {
                        self.rx_length = 0;
                    }
                    self.rx_buffer[self.rx_length..self.rx_length + n]
                        .copy_from_slice(&self.temp[..n]);
                    self.rx_length += n;
                }
                Ok(Err(e)) => return Err(e.into()),
                Err(_) => return Err(ClientError::ReadTimeout),
            }
        }
    }

    pub fn try_read(&mut self) -> Result<Option<CanMessage>, ClientError> {
        let mut parsed_frames = Vec::new();
        let consumed = parse_frames(&self.rx_buffer[..self.rx_length], &mut parsed_frames, false);

        if consumed > 0 {
            self.rx_buffer.copy_within(consumed..self.rx_length, 0);
            self.rx_length -= consumed;
        }

        for parsed in parsed_frames {
            if let Some(msg) = parsed_to_message(&parsed) {
                return Ok(Some(msg));
            }
            error!("Invalid parsed frame (bad id or dlc > 8), dropping");
        }
        Ok(None)
    }
}

/// 便捷函数：直接创建 Client 模式
pub async fn client(config: TokioSerialConfig) -> Result<CanUsbClient, ClientError> {
    CanUsbClient::new(config).await
}
