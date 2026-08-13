//! no_std backend: `embedded-io` transport (sync and async).
//!
//! Same level as the `tokio-serial` backend, but generic over any
//! [`embedded_io`] / [`embedded_io_async`] byte stream (UART, USB-CDC, ...)
//! and over the wire [`Protocol`], with fixed-size buffers and no heap
//! allocation.

use crate::frame::Frame;
#[allow(unused_imports)]
use crate::logging::{debug, trace, Hex};
use crate::message::CanMessage;
use crate::protocol::{ParsedFrameMeta, Protocol};
use embedded_can::{ExtendedId, Id, StandardId};

/// Receive buffer size (bytes)
pub const RX_BUFFER_SIZE: usize = 1024;

/// Transmit scratch buffer size (bytes).
///
/// Must be at least the protocol's `SETTINGS_FRAME_MAX_SIZE` and
/// `DATA_FRAME_MAX_SIZE`; protocol methods validate and error out otherwise.
pub const TX_BUFFER_SIZE: usize = 64;

/// Errors of the embedded-io backend.
#[derive(Debug)]
pub enum EmbeddedIoError<E> {
    /// Underlying IO error
    Io(E),
    /// Protocol error (frame build/parse failure)
    Protocol(&'static str),
}

impl<E: core::fmt::Debug> core::fmt::Display for EmbeddedIoError<E> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            EmbeddedIoError::Io(e) => write!(f, "IO error: {:?}", e),
            EmbeddedIoError::Protocol(e) => write!(f, "Protocol error: {}", e),
        }
    }
}

#[cfg(feature = "std")]
impl<E: core::fmt::Debug> std::error::Error for EmbeddedIoError<E> {}

impl<E> From<E> for EmbeddedIoError<E> {
    fn from(e: E) -> Self {
        EmbeddedIoError::Io(e)
    }
}

/// Convert parsed wire metadata into a [`CanMessage`].
fn meta_to_message(meta: &ParsedFrameMeta, data: &[u8]) -> Option<CanMessage> {
    let can_id = if meta.is_extended {
        Id::Extended(ExtendedId::new(meta.id as u32)?)
    } else {
        Id::Standard(StandardId::new(meta.id)?)
    };
    CanMessage::new(can_id, &data[..meta.dlc as usize])
}

// ============================================
// Blocking (sync) client
// ============================================

/// Blocking CAN adapter client over an [`embedded_io::Read`] + [`embedded_io::Write`] stream.
pub struct CanUsbClient<IO, P: Protocol> {
    io: IO,
    protocol: P,
    config: P::Config,
    debug_traffic: bool,
    rx_buffer: [u8; RX_BUFFER_SIZE],
    rx_length: usize,
}

impl<IO, P> CanUsbClient<IO, P>
where
    IO: ::embedded_io::Read + ::embedded_io::Write,
    P: Protocol,
{
    /// Create a new client and send the adapter settings frame.
    pub fn new(
        mut io: IO,
        protocol: P,
        config: P::Config,
        debug_traffic: bool,
    ) -> Result<Self, EmbeddedIoError<IO::Error>> {
        let mut buf = [0u8; TX_BUFFER_SIZE];
        let len = protocol
            .build_settings_frame(&config, &mut buf)
            .map_err(EmbeddedIoError::Protocol)?;
        io.write_all(&buf[..len])?;
        io.flush()?;
        debug!("Settings sent successfully");

        Ok(Self {
            io,
            protocol,
            config,
            debug_traffic,
            rx_buffer: [0u8; RX_BUFFER_SIZE],
            rx_length: 0,
        })
    }

    /// Get a reference to the underlying IO stream.
    pub fn io(&self) -> &IO {
        &self.io
    }

    /// Get a mutable reference to the underlying IO stream.
    pub fn io_mut(&mut self) -> &mut IO {
        &mut self.io
    }

    /// Get a reference to the protocol configuration.
    pub fn config(&self) -> &P::Config {
        &self.config
    }

    /// Send a CAN frame.
    pub fn write_frame(&mut self, frame: &Frame) -> Result<(), EmbeddedIoError<IO::Error>> {
        let mut buf = [0u8; TX_BUFFER_SIZE];
        let len = self
            .protocol
            .build_data_frame(frame.frame_type, frame.raw_id(), frame.data(), &mut buf)
            .map_err(EmbeddedIoError::Protocol)?;

        if self.debug_traffic {
            trace!("TX: {:?}", Hex(&buf[..len]));
        }

        self.io.write_all(&buf[..len])?;
        self.io.flush()?;
        Ok(())
    }

    /// Parse already-buffered data without touching the IO stream.
    pub fn try_read(&mut self) -> Option<CanMessage> {
        let mut data = [0u8; 8];
        let (consumed, meta) = self.protocol.parse_next_frame(
            &self.rx_buffer[..self.rx_length],
            &mut data,
            self.debug_traffic,
        );

        if consumed > 0 {
            self.rx_buffer.copy_within(consumed..self.rx_length, 0);
            self.rx_length -= consumed;
        }

        meta.and_then(|m| meta_to_message(&m, &data))
    }

    /// Read from the stream once, then try to parse a message.
    ///
    /// Returns `Ok(None)` if no complete frame is available yet.
    pub fn poll_read(&mut self) -> Result<Option<CanMessage>, EmbeddedIoError<IO::Error>> {
        if let Some(msg) = self.try_read() {
            return Ok(Some(msg));
        }

        if self.rx_length >= self.rx_buffer.len() {
            // Buffer full of unparseable data; drop it
            self.rx_length = 0;
        }

        let n = self.io.read(&mut self.rx_buffer[self.rx_length..])?;
        if n > 0 {
            if self.debug_traffic {
                trace!("RX raw: {:?}", Hex(&self.rx_buffer[self.rx_length..self.rx_length + n]));
            }
            self.rx_length += n;
        }

        Ok(self.try_read())
    }

    /// Read until a complete CAN message is received.
    ///
    /// Note: this loops on the underlying blocking `read`; whether it blocks
    /// depends on the IO implementation.
    pub fn read(&mut self) -> Result<CanMessage, EmbeddedIoError<IO::Error>> {
        loop {
            if let Some(msg) = self.poll_read()? {
                return Ok(msg);
            }
        }
    }
}

// ============================================
// Async client
// ============================================

/// Async CAN adapter client over an [`embedded_io_async::Read`] + [`embedded_io_async::Write`] stream.
pub struct AsyncCanUsbClient<IO, P: Protocol> {
    io: IO,
    protocol: P,
    config: P::Config,
    debug_traffic: bool,
    rx_buffer: [u8; RX_BUFFER_SIZE],
    rx_length: usize,
}

impl<IO, P> AsyncCanUsbClient<IO, P>
where
    IO: ::embedded_io_async::Read + ::embedded_io_async::Write,
    P: Protocol,
{
    /// Create a new client and send the adapter settings frame.
    pub async fn new(
        mut io: IO,
        protocol: P,
        config: P::Config,
        debug_traffic: bool,
    ) -> Result<Self, EmbeddedIoError<IO::Error>> {
        let mut buf = [0u8; TX_BUFFER_SIZE];
        let len = protocol
            .build_settings_frame(&config, &mut buf)
            .map_err(EmbeddedIoError::Protocol)?;
        io.write_all(&buf[..len]).await?;
        io.flush().await?;
        debug!("Settings sent successfully");

        Ok(Self {
            io,
            protocol,
            config,
            debug_traffic,
            rx_buffer: [0u8; RX_BUFFER_SIZE],
            rx_length: 0,
        })
    }

    /// Get a reference to the underlying IO stream.
    pub fn io(&self) -> &IO {
        &self.io
    }

    /// Get a mutable reference to the underlying IO stream.
    pub fn io_mut(&mut self) -> &mut IO {
        &mut self.io
    }

    /// Get a reference to the protocol configuration.
    pub fn config(&self) -> &P::Config {
        &self.config
    }

    /// Send a CAN frame.
    pub async fn write_frame(&mut self, frame: &Frame) -> Result<(), EmbeddedIoError<IO::Error>> {
        let mut buf = [0u8; TX_BUFFER_SIZE];
        let len = self
            .protocol
            .build_data_frame(frame.frame_type, frame.raw_id(), frame.data(), &mut buf)
            .map_err(EmbeddedIoError::Protocol)?;

        if self.debug_traffic {
            trace!("TX: {:?}", Hex(&buf[..len]));
        }

        self.io.write_all(&buf[..len]).await?;
        self.io.flush().await?;
        Ok(())
    }

    /// Parse already-buffered data without touching the IO stream.
    pub fn try_read(&mut self) -> Option<CanMessage> {
        let mut data = [0u8; 8];
        let (consumed, meta) = self.protocol.parse_next_frame(
            &self.rx_buffer[..self.rx_length],
            &mut data,
            self.debug_traffic,
        );

        if consumed > 0 {
            self.rx_buffer.copy_within(consumed..self.rx_length, 0);
            self.rx_length -= consumed;
        }

        meta.and_then(|m| meta_to_message(&m, &data))
    }

    /// Wait until a complete CAN message is received.
    pub async fn read(&mut self) -> Result<CanMessage, EmbeddedIoError<IO::Error>> {
        loop {
            if let Some(msg) = self.try_read() {
                return Ok(msg);
            }

            if self.rx_length >= self.rx_buffer.len() {
                // Buffer full of unparseable data; drop it
                self.rx_length = 0;
            }

            let n = self.io.read(&mut self.rx_buffer[self.rx_length..]).await?;
            if n > 0 {
                if self.debug_traffic {
                    trace!("RX raw: {:?}", Hex(&self.rx_buffer[self.rx_length..self.rx_length + n]));
                }
                self.rx_length += n;
            }
        }
    }
}
