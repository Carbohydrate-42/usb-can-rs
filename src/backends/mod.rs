//! Transport backends: how bytes get to and from the USB-CAN adapter.
//!
//! - [`tokio_serial`] (feature `tokio-serial`, std): async serial port transport
//! - [`embedded_io`] (feature `embedded-io`, no_std): sync + async transport
//!   over any `embedded-io` byte stream

#[cfg(feature = "tokio-serial")]
pub mod tokio_serial;

#[cfg(feature = "embedded-io")]
pub mod embedded_io;
