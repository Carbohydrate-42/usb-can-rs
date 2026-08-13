//! Frontends: adaptors that expose the crate through foreign interfaces.
//!
//! - [`zencan`] (feature `zencan`): adaptor for zencan's `BusManager`

#[cfg(feature = "zencan")]
pub mod zencan;
