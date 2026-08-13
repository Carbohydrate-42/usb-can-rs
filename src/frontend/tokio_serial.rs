//! std frontend: tokio-serial transport.
//!
//! Async CAN adapter client over a serial port, in two flavors:
//!
//! - exclusive client ([`CanUsbClient`]) — request/response on one handle,
//!   only one of read or write in progress at a time
//! - split mode ([`CanUsbSender`] + receiver channel) — independent
//!   producer/consumer halves driven by background tasks
//!
//! Both are thin shells over the shared [`Transport`], which owns the serial
//! port, speaks the wire [`Protocol`] and buffers incoming bytes.
//!
//! The serial port is opened by the caller and passed in as a
//! [`tokio_serial::SerialStream`].

#[allow(unused_imports)]
use crate::logging::{error, info, trace, Fmt, Hex};
use crate::message::CanMessage;
use crate::protocol::{ParsedFrameMeta, Protocol};
use embedded_can::{ExtendedId, Frame, Id, StandardId};
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

// ============================================
// Shared transport
// ============================================

/// Serial port + wire protocol + RX buffering, shared by both modes.
struct Transport<P: Protocol> {
    serial: tokio_serial::SerialStream,
    protocol: P,
    rx_buffer: Vec<u8>,
    rx_length: usize,
    temp: Vec<u8>,
}

impl<P: Protocol> Transport<P> {
    /// Create the transport and send the initial settings frame.
    async fn new(
        serial: tokio_serial::SerialStream,
        protocol: P,
        config: &P::Config,
    ) -> Result<Self, ClientError> {
        let mut transport = Self {
            serial,
            protocol,
            rx_buffer: vec![0u8; 1024],
            rx_length: 0,
            temp: vec![0u8; 256],
        };

        transport.send_settings(config).await?;

        Ok(transport)
    }

    async fn send_settings(&mut self, config: &P::Config) -> Result<(), ClientError> {
        let mut buf = [0u8; 64];
        let len = self
            .protocol
            .build_settings_frame(config, &mut buf)
            .map_err(ClientError::Protocol)?;
        self.serial.write_all(&buf[..len]).await?;
        self.serial.flush().await?;
        sleep(Duration::from_millis(100)).await;
        info!("Settings sent successfully");
        Ok(())
    }

    /// Build the wire frame and write it to the serial port.
    async fn write_frame(&mut self, frame: &impl Frame) -> Result<(), ClientError> {
        let mut buf = [0u8; 64];
        let len = self
            .protocol
            .build_data_frame(frame, &mut buf)
            .map_err(ClientError::Protocol)?;

        trace!("TX: {:?}", Hex(&buf[..len]));

        self.serial.write_all(&buf[..len]).await?;
        self.serial.flush().await?;
        Ok(())
    }

    /// Read the next frame, waiting at most `timeout_duration`.
    async fn read_frame(&mut self, timeout_duration: Duration) -> Result<CanMessage, ClientError> {
        let deadline = tokio::time::Instant::now() + timeout_duration;

        loop {
            if let Some(msg) = self.try_read_frame() {
                return Ok(msg);
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
                        "Serial port closed",
                    )));
                }
                Ok(Ok(n)) => {
                    trace!("RX raw: {:?}", Hex(&self.temp[..n]));

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

    /// Parse and return the next buffered frame, if a complete one is available.
    fn try_read_frame(&mut self) -> Option<CanMessage> {
        loop {
            let mut data_out = [0u8; 8];
            let (consumed, meta) = self
                .protocol
                .parse_next_frame(&self.rx_buffer[..self.rx_length], &mut data_out);

            if consumed > 0 {
                self.rx_buffer.copy_within(consumed..self.rx_length, 0);
                self.rx_length -= consumed;
            }

            let meta = meta?;
            if let Some(msg) = Self::parsed_to_message(&meta, &data_out) {
                return Some(msg);
            }
            error!("Invalid parsed frame (bad id or dlc > 8), dropping");
        }
    }

    /// Convert a parsed wire frame into a [`CanMessage`].
    fn parsed_to_message(meta: &ParsedFrameMeta, data: &[u8; 8]) -> Option<CanMessage> {
        let can_id = if meta.is_extended {
            Id::Extended(ExtendedId::new(meta.id as u32)?)
        } else {
            Id::Standard(StandardId::new(meta.id)?)
        };
        CanMessage::new(can_id, &data[..meta.dlc as usize])
    }
}

// ============================================
// Split mode: channel style
// ============================================

/// Sender half - frames are queued into a channel, a background task writes them to the serial port
#[derive(Clone)]
pub struct CanUsbSender {
    frame_tx: mpsc::Sender<CanMessage>,
}

impl CanUsbSender {
    /// Create split mode over an already-opened serial port, returning (sender, receiver)
    ///
    /// The sender can be cloned, supporting multiple concurrent producers.
    /// The receiver is unique and yields incoming CAN messages.
    ///
    /// # Arguments
    /// * `serial` - serial port opened by the caller (e.g. via `open_native_async`)
    /// * `protocol` - wire protocol implementation (e.g. [`crate::protocol::wareshare_usb_can_a::WaveshareUsbCanA`])
    /// * `config` - protocol-specific configuration
    pub async fn split<P>(
        serial: tokio_serial::SerialStream,
        protocol: P,
        config: &P::Config,
    ) -> Result<(Self, mpsc::Receiver<CanMessage>), ClientError>
    where
        P: Protocol + Send + Sync + 'static,
        P::Config: Send + Sync + 'static,
    {
        info!("Creating split mode");

        let transport = Arc::new(Mutex::new(
            Transport::new(serial, protocol, config).await?,
        ));

        // Channel carrying frames to send (user -> background write task)
        let (frame_tx, frame_rx) = mpsc::channel::<CanMessage>(100);
        // Channel carrying received messages (background read task -> user)
        let (msg_tx, msg_rx) = mpsc::channel::<CanMessage>(100);

        // Spawn the background task
        tokio::spawn(Self::background_task(transport, frame_rx, msg_tx));

        Ok((Self { frame_tx }, msg_rx))
    }

    /// Send a CAN frame asynchronously (non-blocking, buffered into the channel)
    ///
    /// Accepts any [`embedded_can::Frame`] implementation.
    pub async fn send(&self, frame: &impl Frame) -> Result<(), ClientError> {
        let msg = Self::to_message(frame).ok_or(ClientError::Protocol("Invalid CAN frame"))?;
        self.frame_tx.send(msg).await.map_err(Into::into)
    }

    /// Try to send without waiting (non-blocking)
    pub fn try_send(&self, frame: &impl Frame) -> Result<(), ClientError> {
        let msg = Self::to_message(frame).ok_or(ClientError::Protocol("Invalid CAN frame"))?;
        self.frame_tx.try_send(msg).map_err(|e| match e {
            mpsc::error::TrySendError::Full(_) => ClientError::Protocol("Send channel full"),
            mpsc::error::TrySendError::Closed(_) => ClientError::ChannelClosed,
        })
    }

    /// Check whether the channel is closed
    pub fn is_closed(&self) -> bool {
        self.frame_tx.is_closed()
    }

    /// Copy any [`embedded_can::Frame`] into an owned [`CanMessage`].
    fn to_message(frame: &impl Frame) -> Option<CanMessage> {
        if frame.is_remote_frame() {
            CanMessage::new_remote(frame.id(), frame.dlc())
        } else {
            CanMessage::new(frame.id(), frame.data())
        }
    }

    /// Background task of split mode: separate read and write tasks
    ///
    /// Two tasks are spawned, one dedicated to writing and one to reading,
    /// exploiting the full-duplex serial port.
    async fn background_task<P>(
        transport: Arc<Mutex<Transport<P>>>,
        mut frame_rx: mpsc::Receiver<CanMessage>,
        msg_tx: mpsc::Sender<CanMessage>,
    ) where
        P: Protocol + Send + Sync + 'static,
    {
        let transport_read = transport.clone();
        let transport_write = transport;

        // Read task
        let read_task = tokio::spawn(async move {
            loop {
                let msg = {
                    let mut transport =
                        match timeout(Duration::from_millis(100), transport_read.lock()).await {
                            Ok(t) => t,
                            Err(_) => continue, // Lock acquisition timed out, retry
                        };

                    match transport.read_frame(Duration::from_millis(50)).await {
                        Ok(msg) => msg,
                        Err(ClientError::ReadTimeout) => continue,
                        Err(_e) => {
                            error!("Serial read error: {}", Fmt(&_e));
                            return;
                        }
                    }
                };

                if msg_tx.send(msg).await.is_err() {
                    error!("Message channel closed, read task exiting");
                    return;
                }
            }
        });

        // Write task
        let write_task = tokio::spawn(async move {
            while let Some(msg) = frame_rx.recv().await {
                let mut transport =
                    match timeout(Duration::from_secs(5), transport_write.lock()).await {
                        Ok(t) => t,
                        Err(_) => {
                            error!("Timeout acquiring serial lock for write");
                            continue;
                        }
                    };

                if let Err(_e) = transport.write_frame(&msg).await {
                    error!("Serial write error: {}", Fmt(&_e));
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
}

// ============================================
// Client mode: exclusive read/write
// ============================================

/// Exclusive client; only one of read or write can be in progress at a time
pub struct CanUsbClient<P: Protocol> {
    transport: Transport<P>,
}

impl<P: Protocol> CanUsbClient<P> {
    /// Create a new client over an already-opened serial port (exclusive ownership)
    pub async fn new(
        serial: tokio_serial::SerialStream,
        protocol: P,
        config: P::Config,
    ) -> Result<Self, ClientError> {
        info!("Creating client mode");

        let transport = Transport::new(serial, protocol, &config).await?;

        Ok(Self { transport })
    }

    /// Write a CAN frame to the adapter.
    ///
    /// Accepts any [`embedded_can::Frame`] implementation.
    pub async fn write(&mut self, frame: &impl Frame) -> Result<(), ClientError> {
        self.transport.write_frame(frame).await
    }

    /// Read the next frame, waiting at most `timeout_duration`.
    pub async fn read(&mut self, timeout_duration: Duration) -> Result<CanMessage, ClientError> {
        self.transport.read_frame(timeout_duration).await
    }

    /// Return the next buffered frame without waiting, if a complete one is available.
    pub fn try_read(&mut self) -> Result<Option<CanMessage>, ClientError> {
        Ok(self.transport.try_read_frame())
    }
}
