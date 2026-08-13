//! USB-CAN-A adapter driver.
//!
//! The core is `no_std` compatible and allocation-free. The wire protocol is
//! abstracted behind [`protocol::Protocol`]; the USB-CAN-A binary protocol
//! ([`protocol::wareshare_usb_can_a::WaveshareUsbCanA`]) is one implementation of it.
//! I/O lives in [`frontend`]:
//!
//! - `tokio-serial` (std): [`frontend::tokio_serial`] — async serial transport
//! - `zencan` (extension): [`frontend::zencan_tokio_serial`] — zencan `BusManager` adaptor
//!
//! Logging backend is selectable via `log` or `defmt` features (mutually
//! exclusive; `defmt` wins if both are set; neither = no logs).

#![cfg_attr(not(feature = "std"), no_std)]

mod logging;

pub mod message;
pub mod protocol;

pub mod frontend;

// Re-export embedded-can CAN types (Frame trait + ID types)
pub use embedded_can::{ExtendedId, Frame, Id, StandardId};

// Re-export our modules
pub use message::CanMessage;
pub use protocol::{ParsedFrameMeta, Protocol};
