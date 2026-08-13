//! Frontends: adaptors that expose the crate through foreign interfaces.
//!
//! - [`zencan_tokio_serial`] (feature `zencan`): adaptor for zencan's `BusManager`

#[cfg(feature = "zencan")]
pub mod zencan_tokio_serial;
#[cfg(feature = "tokio-serial")]
pub mod tokio_serial;
