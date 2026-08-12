//! USB-CAN-A adapter driver.
//!
//! Core protocol is `no_std` compatible (requires `alloc` for the parsing
//! convenience APIs). Transports are provided as optional adapters:
//!
//! - `tokio-serial` (std): [`tokio_serial`] — async serial port transport
//! - `embedded-io` (no_std): [`embedded_io`] — sync + async transport over
//!   any `embedded-io` stream
//! - `zencan` (extension): [`zencan`] — adaptor for zencan's `BusManager`
//!
//! Logging backend is selectable via `log` or `defmt` features (mutually
//! exclusive; `defmt` wins if both are set; neither = no logs).

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

mod logging;

pub mod frame;
pub mod message;
pub mod protocol;
pub mod types;

#[cfg(feature = "tokio-serial")]
pub mod tokio_serial;

#[cfg(feature = "embedded-io")]
pub mod embedded_io;

#[cfg(feature = "zencan")]
pub mod zencan;

// Re-export CAN types from embedded-can
pub use embedded_can::{ExtendedId, Id, StandardId};

// Re-export our modules
pub use frame::Frame;
pub use message::CanMessage;
pub use protocol::{hex_to_bytes, parse_can_id};
pub use types::{CanFrameType, CanMode, CanSpeed, CanUsbConfig, InvalidCanSpeed, PayloadMode};
