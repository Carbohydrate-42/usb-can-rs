//! Common type definitions shared by all protocols and adapters.
//!
//! CAN message types come from [`embedded_can`] (see [`crate::message`]);
//! protocol-specific configuration types live in their protocol
//! implementation (e.g. [`crate::protocol::wareshare_usb_can_a`]).

// Re-export embedded-can ID types for convenience
pub use embedded_can::{ExtendedId, Id, StandardId};

/// CAN frame type (Standard or Extended)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CanFrameType {
    /// Standard frame (11-bit ID)
    Standard = 0x01,
    /// Extended frame (29-bit ID)
    Extended = 0x02,
}

/// Payload injection mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PayloadMode {
    /// Random payload
    Random,
    /// Incremental payload
    Incremental,
    /// Fixed payload (default)
    #[default]
    Fixed,
}
