//! no_std adapter: `embedded-io` transport (sync and async).
//!
//! Same level as the `tokio-serial` adapter, but generic over any
//! [`embedded_io`] / [`embedded_io_async`] byte stream (UART, USB-CDC, ...),
//! with fixed-size buffers and no heap allocation.

use crate::frame::Frame;
#[allow(unused_imports)]
use crate::logging::{debug, trace, Hex};
use crate::message::CanMessage;
use crate::protocol::{
    build_data_frame_into, build_settings_frame, parse_next_frame, ParsedFrameMeta,
    DATA_FRAME_MAX_SIZE,
};
use crate::types::CanUsbConfig;
use embedded_can::{ExtendedId, Id, StandardId};

/// Receive buffer size (bytes)
pub const RX_BUFFER_SIZE: usize = 1024;

/// Errors of the embedded-io adapter.
#[derive(Debug)]
pub enum EmbeddedIoError<E> {
    /// Underlying IO error
    Io(E),
    /// Frame data too long (max 8 bytes)
    FrameTooLarge,
}

impl<E: core::fmt::Debug> core::fmt::Display for EmbeddedIoError<E> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            EmbeddedIoError::Io(e) => write!(f, "IO error: {:?}", e),
            EmbeddedIoError::FrameTooLarge => write!(f, "Frame too large"),
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

fn build_frame_bytes(frame: &Frame, out: &mut [u8; DATA_FRAME_MAX_SIZE]) -> Result<usize, &'static str> {
    build_data_frame_into(frame.frame_type, frame.raw_id(), frame.data(), out)
}

// ============================================
// Blocking (sync) client
// ============================================

/// Blocking CAN USB client over an [`embedded_io::Read`] + [`embedded_io::Write`] stream.
pub struct CanUsbClient<IO> {
    io: IO,
    config: CanUsbConfig,
    rx_buffer: [u8; RX_BUFFER_SIZE],
    rx_length: usize,
}

impl<IO> CanUsbClient<IO>
where
    IO: ::embedded_io::Read + ::embedded_io::Write,
{
    /// Create a new client and send the adapter settings frame.
    pub fn new(mut io: IO, config: CanUsbConfig) -> Result<Self, EmbeddedIoError<IO::Error>> {
        let settings = build_settings_frame(
            config.can_speed as u8,
            config.can_mode as u8,
            config.frame_type as u8,
            config.filter_id,
            config.mask_id,
        );
        io.write_all(&settings)?;
        io.flush()?;
        debug!("Settings sent successfully");

        Ok(Self {
            io,
            config,
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

    /// Send a CAN frame.
    pub fn write_frame(&mut self, frame: &Frame) -> Result<(), EmbeddedIoError<IO::Error>> {
        let mut buf = [0u8; DATA_FRAME_MAX_SIZE];
        let len = build_frame_bytes(frame, &mut buf).map_err(|_| EmbeddedIoError::FrameTooLarge)?;

        if self.config.debug_traffic {
            trace!("TX: {:?}", Hex(&buf[..len]));
        }

        self.io.write_all(&buf[..len])?;
        self.io.flush()?;
        Ok(())
    }

    /// Parse already-buffered data without touching the IO stream.
    pub fn try_read(&mut self) -> Option<CanMessage> {
        let mut data = [0u8; 8];
        let (consumed, meta) = parse_next_frame(
            &self.rx_buffer[..self.rx_length],
            &mut data,
            self.config.debug_traffic,
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
            if self.config.debug_traffic {
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

/// Async CAN USB client over an [`embedded_io_async::Read`] + [`embedded_io_async::Write`] stream.
pub struct AsyncCanUsbClient<IO> {
    io: IO,
    config: CanUsbConfig,
    rx_buffer: [u8; RX_BUFFER_SIZE],
    rx_length: usize,
}

impl<IO> AsyncCanUsbClient<IO>
where
    IO: ::embedded_io_async::Read + ::embedded_io_async::Write,
{
    /// Create a new client and send the adapter settings frame.
    pub async fn new(mut io: IO, config: CanUsbConfig) -> Result<Self, EmbeddedIoError<IO::Error>> {
        let settings = build_settings_frame(
            config.can_speed as u8,
            config.can_mode as u8,
            config.frame_type as u8,
            config.filter_id,
            config.mask_id,
        );
        io.write_all(&settings).await?;
        io.flush().await?;
        debug!("Settings sent successfully");

        Ok(Self {
            io,
            config,
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

    /// Send a CAN frame.
    pub async fn write_frame(&mut self, frame: &Frame) -> Result<(), EmbeddedIoError<IO::Error>> {
        let mut buf = [0u8; DATA_FRAME_MAX_SIZE];
        let len = build_frame_bytes(frame, &mut buf).map_err(|_| EmbeddedIoError::FrameTooLarge)?;

        if self.config.debug_traffic {
            trace!("TX: {:?}", Hex(&buf[..len]));
        }

        self.io.write_all(&buf[..len]).await?;
        self.io.flush().await?;
        Ok(())
    }

    /// Parse already-buffered data without touching the IO stream.
    pub fn try_read(&mut self) -> Option<CanMessage> {
        let mut data = [0u8; 8];
        let (consumed, meta) = parse_next_frame(
            &self.rx_buffer[..self.rx_length],
            &mut data,
            self.config.debug_traffic,
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
                if self.config.debug_traffic {
                    trace!("RX raw: {:?}", Hex(&self.rx_buffer[self.rx_length..self.rx_length + n]));
                }
                self.rx_length += n;
            }
        }
    }
}
