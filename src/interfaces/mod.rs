//! Interfaces: adaptors that expose the crate through foreign CAN ecosystem
//! interfaces (frame-level, backend-agnostic in intent).
//!
//! - [`zencan_tokio_serial`] (feature `zencan`): adaptor for zencan's `BusManager`

#[cfg(feature = "zencan")]
pub mod zencan_tokio_serial;
