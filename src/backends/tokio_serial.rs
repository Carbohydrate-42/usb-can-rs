//! std backend: tokio-serial transport.
//!
//! Async CAN adapter client over a serial port, supporting both a
//! channel-style split mode and an exclusive client mode.
//!
//! The serial port is opened by the caller and passed in as a
//! [`tokio_serial::SerialStream`]; the backend is generic over the wire
//! [`Protocol`].

use crate::frame::Frame;
#[allow(unused_imports)]
use crate::logging::{error, info, trace, Fmt, Hex};
use crate::message::CanMessage;
use crate::protocol::{ParsedFrame, Protocol};
use embedded_can::{ExtendedId, Id, StandardId};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, Mutex};
use tokio::time::{sleep, timeout};

/// Client errors
#[derive(Debug)]
pub enum ClientError {
    /// Serial port error
    Serial(tokio_serial::Error),
    /// IO error
    Io(std::io::Error),
    /// Protocol error (frame build/parse failure)
    Protocol(&'static str),
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
            ClientError::Protocol(e) => write!(f, "Protocol error: {}", e),
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
fn parsed_to_message(parsed: &ParsedFrame) -> Option<CanMessage> {
    let can_id = if parsed.is_extended {
        Id::Extended(ExtendedId::new(parsed.id as u32)?)
    } else {
        Id::Standard(StandardId::new(parsed.id)?)
    };
    CanMessage::new(can_id, &parsed.data)
}

// ============================================
// Split mode: channel style
// ============================================

/// Sender half - frames are queued into a channel, a background task writes them to the serial port
#[derive(Clone)]
pub struct CanUsbSender {
    frame_tx: mpsc::Sender<Frame>,
}

impl CanUsbSender {
    /// Send a CAN frame asynchronously (non-blocking, buffered into the channel)
    pub async fn send(&self, frame: Frame) -> Result<(), ClientError> {
        self.frame_tx.send(frame).await.map_err(Into::into)
    }

    /// Try to send without waiting (non-blocking)
    pub fn try_send(&self, frame: Frame) -> Result<(), mpsc::error::TrySendError<Frame>> {
        self.frame_tx.try_send(frame)
    }

    /// Check whether the channel is closed
    pub fn is_closed(&self) -> bool {
        self.frame_tx.is_closed()
    }
}

/// Create split mode over an already-opened serial port, returning (sender, receiver)
///
/// The sender can be cloned, supporting multiple concurrent producers.
/// The receiver is unique and yields incoming CAN messages.
///
/// # Arguments
/// * `serial` - serial port opened by the caller (e.g. via `open_native_async`)
/// * `protocol` - wire protocol implementation (e.g. [`crate::WaveshareUsbCanA`])
/// * `config` - protocol-specific configuration
/// * `debug_traffic` - log raw wire traffic
pub async fn split<P>(
    serial: tokio_serial::SerialStream,
    protocol: P,
    config: &P::Config,
    debug_traffic: bool,
) -> Result<(CanUsbSender, mpsc::Receiver<CanMessage>), ClientError>
where
    P: Protocol + Send + Sync + 'static,
    P::Config: Send + Sync + 'static,
{
    info!("Creating split mode");

    let serial = Arc::new(Mutex::new(serial));

    // Channel carrying frames to send (user -> background write task)
    let (frame_tx, frame_rx) = mpsc::channel::<Frame>(100);
    // Channel carrying received messages (background read task -> user)
    let (msg_tx, msg_rx) = mpsc::channel::<CanMessage>(100);

    // Send the initial settings frame
    {
        let mut buf = [0u8; 64];
        let len = protocol
            .build_settings_frame(config, &mut buf)
            .map_err(ClientError::Protocol)?;
        let mut port = serial.lock().await;
        port.write_all(&buf[..len]).await?;
        port.flush().await?;
        sleep(Duration::from_millis(100)).await;
    }
    info!("Settings sent successfully");

    // Spawn the background task
    tokio::spawn(split_background_task(
        serial,
        protocol,
        frame_rx,
        msg_tx,
        debug_traffic,
    ));

    let sender = CanUsbSender { frame_tx };

    Ok((sender, msg_rx))
}

/// Background task of split mode: separate read and write tasks
///
/// Two tasks are spawned, one dedicated to writing and one to reading,
/// avoiding Mutex contention and exploiting the full-duplex serial port.
async fn split_background_task<P>(
    serial: Arc<Mutex<tokio_serial::SerialStream>>,
    protocol: P,
    mut frame_rx: mpsc::Receiver<Frame>,
    msg_tx: mpsc::Sender<CanMessage>,
    debug_traffic: bool,
) where
    P: Protocol + Send + Sync + 'static,
{
    // No coordination channel between the two tasks is needed:
    // the serial port is full-duplex, so reads and writes run truly in parallel.

    let serial_read = serial.clone();
    let serial_write = serial;

    let protocol = Arc::new(protocol);
    let protocol_read = protocol.clone();
    let protocol_write = protocol;

    // Read task
    let read_task = tokio::spawn(async move {
        let mut rx_buffer = vec![0u8; 1024];
        let mut rx_length: usize = 0;
        let mut temp = vec![0u8; 256];

        loop {
            // Acquire the lock and read
            let bytes_read = {
                let mut port = match timeout(Duration::from_millis(100), serial_read.lock()).await {
                    Ok(p) => p,
                    Err(_) => continue, // Lock acquisition timed out, retry
                };

                match timeout(Duration::from_millis(50), port.read(&mut temp)).await {
                    Ok(Ok(n)) => n,
                    Ok(Err(_e)) => {
                        error!("Serial read error: {}", Fmt(&_e));
                        return;
                    }
                    Err(_) => 0, // Read timeout
                }
            };

            if bytes_read == 0 {
                sleep(Duration::from_micros(100)).await;
                continue;
            }

            if debug_traffic {
                trace!("RX raw: {:?}", Hex(&temp[..bytes_read]));
            }

            // Copy into the buffer
            if rx_length + bytes_read > rx_buffer.len() {
                rx_length = 0;
            }
            rx_buffer[rx_length..rx_length + bytes_read]
                .copy_from_slice(&temp[..bytes_read]);
            rx_length += bytes_read;

            // Parse frames
            let mut parsed_frames = Vec::new();
            let consumed = protocol_read.parse_frames(
                &rx_buffer[..rx_length],
                &mut parsed_frames
            );

            if consumed > 0 {
                rx_buffer.copy_within(consumed..rx_length, 0);
                rx_length -= consumed;
            }

            // Forward messages
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

    // Write task
    let write_task = tokio::spawn(async move {
        while let Some(frame) = frame_rx.recv().await {
            // Build the wire data frame
            let mut buf = [0u8; 64];
            let len = match protocol_write.build_data_frame(
                frame.frame_type,
                frame.raw_id(),
                frame.data(),
                &mut buf,
            ) {
                Ok(n) => n,
                Err(_) => {
                    error!("Frame too large, dropping");
                    continue;
                }
            };

            // Acquire the lock and write
            let mut port = match timeout(Duration::from_secs(5), serial_write.lock()).await {
                Ok(p) => p,
                Err(_) => {
                    error!("Timeout acquiring serial lock for write");
                    continue;
                }
            };

            if debug_traffic {
                trace!("TX: {:?}", Hex(&buf[..len]));
            }

            if let Err(_e) = port.write_all(&buf[..len]).await {
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

    // Wait until either task finishes
    tokio::select! {
        _ = read_task => error!("Read task exited"),
        _ = write_task => error!("Write task exited"),
    }

    error!("Split background task exited!");
}

// ============================================
// Client mode: exclusive read/write
// ============================================

/// Exclusive client; only one of read or write can be in progress at a time
pub struct CanUsbClient<P: Protocol> {
    serial: tokio_serial::SerialStream,
    protocol: P,
    config: P::Config,
    debug_traffic: bool,
    rx_buffer: Vec<u8>,
    rx_length: usize,
    temp: Vec<u8>,
}

impl<P: Protocol> CanUsbClient<P> {
    /// Create a new client over an already-opened serial port (exclusive ownership)
    pub async fn new(
        serial: tokio_serial::SerialStream,
        protocol: P,
        config: P::Config,
        debug_traffic: bool,
    ) -> Result<Self, ClientError> {
        info!("Creating client mode");

        let mut client = Self {
            serial,
            protocol,
            config,
            debug_traffic,
            rx_buffer: vec![0u8; 1024],
            rx_length: 0,
            temp: vec![0u8; 256],
        };

        client.send_settings().await?;

        Ok(client)
    }

    async fn send_settings(&mut self) -> Result<(), ClientError> {
        let mut buf = [0u8; 64];
        let len = self
            .protocol
            .build_settings_frame(&self.config, &mut buf)
            .map_err(ClientError::Protocol)?;
        self.serial.write_all(&buf[..len]).await?;
        self.serial.flush().await?;
        sleep(Duration::from_millis(100)).await;
        info!("Settings sent successfully");
        Ok(())
    }

    pub async fn write(&mut self, frame: &Frame) -> Result<(), ClientError> {
        let mut buf = [0u8; 64];
        let len = self
            .protocol
            .build_data_frame(frame.frame_type, frame.raw_id(), frame.data(), &mut buf)
            .map_err(ClientError::Protocol)?;

        if self.debug_traffic {
            trace!("TX: {:?}", Hex(&buf[..len]));
        }

        self.serial.write_all(&buf[..len]).await?;
        self.serial.flush().await?;
        Ok(())
    }

    pub async fn read(&mut self, timeout_duration: Duration) -> Result<CanMessage, ClientError> {
        let deadline = tokio::time::Instant::now() + timeout_duration;

        loop {
            // Try parsing the buffer first
            let mut parsed_frames = Vec::new();
            let consumed = self.protocol.parse_frames(
                &self.rx_buffer[..self.rx_length],
                &mut parsed_frames,
            );

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
                    if self.debug_traffic {
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
        let consumed = self.protocol.parse_frames(
            &self.rx_buffer[..self.rx_length],
            &mut parsed_frames,
        );

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

/// Convenience function: create a client directly
pub async fn client<P: Protocol>(
    serial: tokio_serial::SerialStream,
    protocol: P,
    config: P::Config,
    debug_traffic: bool,
) -> Result<CanUsbClient<P>, ClientError> {
    CanUsbClient::new(serial, protocol, config, debug_traffic).await
}
