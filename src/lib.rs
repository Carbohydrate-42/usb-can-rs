//! USB-CAN-A adapter driver.
//!
//! The core is `no_std` compatible (requires `alloc` for the parsing
//! convenience APIs). The wire protocol is abstracted behind
//! [`protocol::Protocol`]; the USB-CAN-A binary protocol
//! ([`protocol::wareshare_usb_can_a::WaveshareUsbCanA`]) is one implementation of it.
//! I/O is organized in two layers:
//!
//! - [`backends`] — transports that move bytes to/from the adapter:
//!   - `tokio-serial` (std): [`backends::tokio_serial`]
//!   - `embedded-io` (no_std, sync + async): [`backends::embedded_io`]
//! - [`frontends`] — adaptors exposing foreign interfaces:
//!   - `zencan` (extension): [`frontends::zencan`]
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

pub mod backends;
pub mod frontends;

// Re-export CAN types from embedded-can
pub use embedded_can::{ExtendedId, Id, StandardId};

// Re-export our modules
pub use frame::Frame;
pub use message::CanMessage;
pub use protocol::{hex_to_bytes, parse_can_id, ParsedFrame, ParsedFrameMeta, Protocol};
pub use types::{CanFrameType, PayloadMode};
