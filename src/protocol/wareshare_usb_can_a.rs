//! USB-CAN-A wire protocol implementation of [`Protocol`].

#[allow(unused_imports)]
use crate::logging::{debug, Hex};
use crate::protocol::{ParsedFrameMeta, Protocol};
use alloc::vec::Vec;
use crate::types::CanFrameType;

/// USB-CAN command frame size
pub const CMD_FRAME_SIZE: usize = 20;
/// USB-CAN data frame header size (0xAA + info + id)
pub const DATA_FRAME_HEADER_SIZE: usize = 4;
/// USB-CAN data frame footer byte
pub const FRAME_FOOTER: u8 = 0x55;
/// USB-CAN command frame marker
pub const CMD_FRAME_MARKER: u8 = 0x55;
/// USB-CAN frame start byte
pub const FRAME_START: u8 = 0xAA;
/// Max wire size of a data frame: header(2) + id(2) + data(8) + footer(1)
pub const DATA_FRAME_MAX_SIZE: usize = 13;

// ============================================
// Protocol-specific configuration types
// ============================================

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

/// Configuration of the USB-CAN-A adapter.
#[derive(Debug, Clone)]
pub struct Config {
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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            can_speed: CanSpeed::Bps500000,
            can_mode: CanMode::Normal,
            frame_type: CanFrameType::Standard,
            filter_id: 0,
            mask_id: 0,
        }
    }
}

// ============================================
// Protocol implementation
// ============================================

/// USB-CAN-A wire protocol.
#[derive(Debug, Clone, Copy, Default)]
pub struct WaveshareUsbCanA;

impl WaveshareUsbCanA {
    // ============================================
    // Free functions (usable without the trait)
    // ============================================

    /// Build a settings/command frame for USB-CAN adapter configuration
    ///
    /// Frame format (20 bytes):
    /// - Byte 0: 0xAA (start)
    /// - Byte 1: 0x55 (command marker)
    /// - Byte 2: 0x12 (command type)
    /// - Byte 3: speed
    /// - Byte 4: frame_type
    /// - Byte 5-8: filter_id (little endian)
    /// - Byte 9-12: mask_id (little endian)
    /// - Byte 13: mode
    /// - Byte 14: 0x01
    /// - Byte 15-18: reserved (0)
    /// - Byte 19: checksum
    pub fn build_settings_frame(
        speed: u8,
        mode: u8,
        frame_type: u8,
        filter_id: u32,
        mask_id: u32,
    ) -> [u8; CMD_FRAME_SIZE] {
        let mut frame = [0u8; CMD_FRAME_SIZE];

        frame[0] = FRAME_START;
        frame[1] = CMD_FRAME_MARKER;
        frame[2] = 0x12;
        frame[3] = speed;
        frame[4] = frame_type;

        // Filter ID (byte 5-8) - little endian
        frame[5] = (filter_id & 0xFF) as u8;
        frame[6] = ((filter_id >> 8) & 0xFF) as u8;
        frame[7] = ((filter_id >> 16) & 0xFF) as u8;
        frame[8] = ((filter_id >> 24) & 0xFF) as u8;

        // Mask ID (byte 9-12) - little endian
        frame[9] = (mask_id & 0xFF) as u8;
        frame[10] = ((mask_id >> 8) & 0xFF) as u8;
        frame[11] = ((mask_id >> 16) & 0xFF) as u8;
        frame[12] = ((mask_id >> 24) & 0xFF) as u8;

        frame[13] = mode;
        frame[14] = 0x01;
        // Byte 15-18 are already 0

        frame[19] = Self::generate_checksum(&frame, 2, 17);

        frame
    }

    /// Build a data frame for transmission into a caller-provided buffer (no-alloc).
    ///
    /// Frame format:
    /// - Byte 0: 0xAA (start)
    /// - Byte 1: 0xC0 | (is_extended << 5) | dlc
    /// - Byte 2-3: ID (little endian)
    /// - Byte 4..4+dlc: Data
    /// - Byte 4+dlc: 0x55 (footer)
    ///
    /// Returns the number of bytes written to `out`.
    pub fn build_data_frame_into(
        frame_type: CanFrameType,
        id: u32,
        data: &[u8],
        out: &mut [u8; DATA_FRAME_MAX_SIZE],
    ) -> Result<usize, &'static str> {
        if data.len() > 8 {
            return Err("Data too long (max 8 bytes)");
        }

        // Start byte
        out[0] = FRAME_START;

        // Info byte: 0xC0 | extended_flag | dlc
        let mut info: u8 = 0xC0;
        if frame_type == CanFrameType::Extended {
            info |= 0x20;
        }
        info |= data.len() as u8;
        out[1] = info;

        // ID (little endian)
        out[2] = (id & 0xFF) as u8;
        out[3] = ((id >> 8) & 0xFF) as u8;

        // Data
        out[4..4 + data.len()].copy_from_slice(data);

        // Footer
        out[4 + data.len()] = FRAME_FOOTER;

        Ok(5 + data.len())
    }

    /// Build a data frame for transmission
    ///
    /// Allocating variant of [`build_data_frame_into`].
    pub fn build_data_frame(
        frame_type: CanFrameType,
        id: impl Into<u32>,
        data: &[u8],
    ) -> Result<Vec<u8>, &'static str> {
        let mut out = [0u8; DATA_FRAME_MAX_SIZE];
        let len = Self::build_data_frame_into(frame_type, id.into(), data, &mut out)?;
        Ok(out[..len].to_vec())
    }

    /// Generate checksum for command frames
    ///
    /// Sum of bytes from offset to offset+length, masked to 8 bits
    pub fn generate_checksum(data: &[u8], offset: usize, length: usize) -> u8 {
        let sum: usize = data[offset..offset + length]
            .iter()
            .map(|&b| b as usize)
            .sum();
        (sum & 0xFF) as u8
    }

    /// Scan `buffer` for the next complete frame, skipping junk bytes.
    ///
    /// Returns `(consumed, frame)`:
    /// - `consumed` is the number of bytes that can be dropped from the front of
    ///   the buffer (junk bytes, plus the frame itself if one was found). When an
    ///   incomplete frame is found at `offset`, only the junk before it is reported
    ///   as consumed so the partial frame stays in the buffer.
    /// - `frame` is `Some(meta)` when a complete frame was parsed; its data
    ///   (0-8 bytes) is copied into `data_out`. Frames with DLC > 8 are treated
    ///   as junk.
    pub fn parse_next_frame(
        buffer: &[u8],
        data_out: &mut [u8; 8],
        debug_traffic: bool,
    ) -> (usize, Option<ParsedFrameMeta>) {
        let mut offset = 0;

        while buffer.len() - offset >= 6 {
            // Look for frame start
            if buffer[offset] != FRAME_START {
                offset += 1;
                continue;
            }

            let info = buffer[offset + 1];

            // Check if it's a data frame (0xC0-0xCF for standard, 0xE0-0xEF for extended)
            let frame_type_nibble = info >> 4;
            if frame_type_nibble != 0x0C && frame_type_nibble != 0x0E {
                offset += 1;
                continue;
            }

            let dlc = (info & 0x0F) as usize;
            if dlc > 8 {
                // Not a valid classic-CAN frame; treat start byte as junk
                offset += 1;
                continue;
            }
            let frame_len = dlc + 5; // header(2) + id(2) + data(dlc) + footer(1)

            // Check if we have the complete frame
            if buffer.len() - offset < frame_len {
                break; // Wait for more data
            }

            // Verify end byte
            if buffer[offset + frame_len - 1] != FRAME_FOOTER {
                offset += 1;
                continue;
            }

            // Extract ID (little endian)
            let id = u16::from_le_bytes([buffer[offset + 2], buffer[offset + 3]]);

            // Extract data
            let data_start = offset + 4;
            data_out[..dlc].copy_from_slice(&buffer[data_start..data_start + dlc]);

            // Determine frame type
            let is_extended = (info & 0x20) != 0;

            if debug_traffic {
                debug!(
                    "Parsed frame: id=0x{:03x}, extended={}, dlc={}, data={:?}",
                    id,
                    is_extended,
                    dlc,
                    Hex(&data_out[..dlc])
                );
            }

            return (
                offset + frame_len,
                Some(ParsedFrameMeta {
                    id,
                    dlc: dlc as u8,
                    is_extended,
                }),
            );
        }

        (offset, None)
    }
}

impl Protocol for WaveshareUsbCanA {
    type Config = Config;

    const SETTINGS_FRAME_MAX_SIZE: usize = CMD_FRAME_SIZE;
    const DATA_FRAME_MAX_SIZE: usize = DATA_FRAME_MAX_SIZE;

    fn build_settings_frame(
        &self,
        config: &Self::Config,
        out: &mut [u8],
    ) -> Result<usize, &'static str> {
        if out.len() < CMD_FRAME_SIZE {
            return Err("Output buffer too small for settings frame");
        }
        let frame = Self::build_settings_frame(
            config.can_speed as u8,
            config.can_mode as u8,
            config.frame_type as u8,
            config.filter_id,
            config.mask_id,
        );
        out[..CMD_FRAME_SIZE].copy_from_slice(&frame);
        Ok(CMD_FRAME_SIZE)
    }

    fn build_data_frame(
        &self,
        frame_type: CanFrameType,
        id: u32,
        data: &[u8],
        out: &mut [u8],
    ) -> Result<usize, &'static str> {
        if out.len() < DATA_FRAME_MAX_SIZE {
            return Err("Output buffer too small for data frame");
        }
        let out: &mut [u8; DATA_FRAME_MAX_SIZE] =
            (&mut out[..DATA_FRAME_MAX_SIZE]).try_into().unwrap();
        Self::build_data_frame_into(frame_type, id, data, out)
    }

    fn parse_next_frame(
        &self,
        buffer: &[u8],
        data_out: &mut [u8; 8],
        debug_traffic: bool,
    ) -> (usize, Option<ParsedFrameMeta>) {
        Self::parse_next_frame(buffer, data_out, debug_traffic)
    }
}
