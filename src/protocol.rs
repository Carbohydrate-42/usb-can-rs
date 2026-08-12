//! USB-CAN protocol implementation

#[allow(unused_imports)]
use crate::logging::{debug, Hex};
use crate::message::id_from_raw;
use crate::types::CanFrameType;
use alloc::string::String;
use alloc::vec::Vec;

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

    frame[19] = generate_checksum(&frame, 2, 17);

    frame
}

/// Max wire size of a data frame: header(2) + id(2) + data(8) + footer(1)
pub const DATA_FRAME_MAX_SIZE: usize = 13;

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
    let len = build_data_frame_into(frame_type, id.into(), data, &mut out)?;
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

/// A parsed CAN frame from the wire
#[derive(Debug, Clone)]
pub struct ParsedFrame {
    /// Frame ID (11-bit or 29-bit)
    pub id: u16,
    /// Frame data (0-8 bytes)
    pub data: Vec<u8>,
    /// True if extended frame (29-bit ID)
    pub is_extended: bool,
}

/// Metadata of a single parsed frame (no-alloc variant).
///
/// Frame data is copied into the caller-provided buffer by [`parse_next_frame`].
#[derive(Debug, Clone, Copy)]
pub struct ParsedFrameMeta {
    /// Frame ID (11-bit or 29-bit)
    pub id: u16,
    /// Data Length Code (0-8)
    pub dlc: u8,
    /// True if extended frame (29-bit ID)
    pub is_extended: bool,
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
                id, is_extended, dlc, Hex(&data_out[..dlc])
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

/// Parse incoming buffer and extract complete frames
///
/// Returns the number of bytes consumed from the buffer
pub fn parse_frames(
    buffer: &[u8],
    output: &mut Vec<ParsedFrame>,
    debug_traffic: bool,
) -> usize {
    let mut total_consumed = 0;
    let mut data_out = [0u8; 8];

    loop {
        let (consumed, frame) = parse_next_frame(&buffer[total_consumed..], &mut data_out, debug_traffic);
        total_consumed += consumed;

        match frame {
            Some(meta) => output.push(ParsedFrame {
                id: meta.id,
                data: data_out[..meta.dlc as usize].to_vec(),
                is_extended: meta.is_extended,
            }),
            None => break,
        }
    }

    total_consumed
}

/// Convert hex string to binary data
///
/// # Arguments
/// * `hex` - Hex string (e.g., "DEADBEEF", spaces allowed)
///
/// # Returns
/// Vector of bytes
pub fn hex_to_bytes(hex: &str) -> Option<Vec<u8>> {
    let hex: String = hex.chars().filter(|c| c.is_ascii_hexdigit()).collect();

    if hex.len() % 2 != 0 {
        return None;
    }

    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect()
}

/// Parse a CAN ID from hex string
///
/// Supports 1-8 character hex strings for both standard (11-bit) and extended (29-bit) IDs
pub fn parse_can_id(hex_id: &str) -> Option<embedded_can::Id> {
    let hex: String = hex_id.chars().filter(|c| c.is_ascii_hexdigit()).collect();

    // Standard IDs are 11-bit (max 0x7FF), Extended IDs are 29-bit
    let id = u32::from_str_radix(&hex, 16).ok()?;

    id_from_raw(id)
}
