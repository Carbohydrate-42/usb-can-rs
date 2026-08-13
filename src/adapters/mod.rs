//! Adapters: device backends that move CAN frames to/from the adapter
//! hardware.
//!
//! - [`tokio_serial`] (feature `tokio-serial`): Waveshare USB-CAN-A over a
//!   serial port

#[cfg(feature = "tokio-serial")]
pub mod tokio_serial;
