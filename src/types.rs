//! Type definitions for USB-CAN adapter
//!
//! CAN message types come from [`embedded_can`] (see [`crate::message`]);
//! this module adds USB-CAN adapter specific types.

// Re-export embedded-can ID types for convenience
pub use embedded_can::{ExtendedId, Id, StandardId};

/// CAN bus speed options
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CanSpeed {
    /// 1 Mbps
    Bps1000000 = 0x01,
    /// 800 Kbps
    Bps800000 = 0x02,
    /// 500 Kbps
    Bps500000 = 0x03,
    /// 400 Kbps
    Bps400000 = 0x04,
    /// 250 Kbps
    Bps250000 = 0x05,
    /// 200 Kbps
    Bps200000 = 0x06,
    /// 125 Kbps
    Bps125000 = 0x07,
    /// 100 Kbps
    Bps100000 = 0x08,
    /// 50 Kbps
    Bps50000 = 0x09,
    /// 20 Kbps
    Bps20000 = 0x0a,
    /// 10 Kbps
    Bps10000 = 0x0b,
    /// 5 Kbps
    Bps5000 = 0x0c,
}

/// Error when parsing CAN speed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidCanSpeed(pub u32);

impl core::fmt::Display for InvalidCanSpeed {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Invalid CAN speed: {}", self.0)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for InvalidCanSpeed {}

impl TryFrom<u32> for CanSpeed {
    type Error = InvalidCanSpeed;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            1000000 => Ok(CanSpeed::Bps1000000),
            800000 => Ok(CanSpeed::Bps800000),
            500000 => Ok(CanSpeed::Bps500000),
            400000 => Ok(CanSpeed::Bps400000),
            250000 => Ok(CanSpeed::Bps250000),
            200000 => Ok(CanSpeed::Bps200000),
            125000 => Ok(CanSpeed::Bps125000),
            100000 => Ok(CanSpeed::Bps100000),
            50000 => Ok(CanSpeed::Bps50000),
            20000 => Ok(CanSpeed::Bps20000),
            10000 => Ok(CanSpeed::Bps10000),
            5000 => Ok(CanSpeed::Bps5000),
            _ => Err(InvalidCanSpeed(value)),
        }
    }
}

impl CanSpeed {
    /// Get speed in bits per second
    pub fn as_bps(&self) -> u32 {
        match self {
            CanSpeed::Bps1000000 => 1000000,
            CanSpeed::Bps800000 => 800000,
            CanSpeed::Bps500000 => 500000,
            CanSpeed::Bps400000 => 400000,
            CanSpeed::Bps250000 => 250000,
            CanSpeed::Bps200000 => 200000,
            CanSpeed::Bps125000 => 125000,
            CanSpeed::Bps100000 => 100000,
            CanSpeed::Bps50000 => 50000,
            CanSpeed::Bps20000 => 20000,
            CanSpeed::Bps10000 => 10000,
            CanSpeed::Bps5000 => 5000,
        }
    }
}

/// CAN bus operation mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CanMode {
    /// Normal mode
    Normal = 0x00,
    /// Loopback mode
    Loopback = 0x01,
    /// Silent mode
    Silent = 0x02,
    /// Loopback + Silent mode
    LoopbackSilent = 0x03,
}

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

/// CAN-side configuration for the USB-CAN adapter.
///
/// Transport-specific settings (serial device, baudrate, ...) live in the
/// respective adapter crates/modules (e.g. [`crate::tokio_serial`]).
#[derive(Debug, Clone)]
pub struct CanUsbConfig {
    /// CAN bus speed
    pub can_speed: CanSpeed,
    /// CAN operation mode
    pub can_mode: CanMode,
    /// CAN frame type
    pub frame_type: CanFrameType,
    /// Filter ID (default: 0 - no filtering)
    pub filter_id: u32,
    /// Mask ID (default: 0 - no masking)
    pub mask_id: u32,
    /// Print traffic debugging info
    pub debug_traffic: bool,
}

impl Default for CanUsbConfig {
    fn default() -> Self {
        Self {
            can_speed: CanSpeed::Bps500000,
            can_mode: CanMode::Normal,
            frame_type: CanFrameType::Standard,
            filter_id: 0,
            mask_id: 0,
            debug_traffic: false,
        }
    }
}
