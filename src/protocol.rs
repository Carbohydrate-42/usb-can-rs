//! USB-CAN protocol implementation

use crate::types::CanFrameType;
use tracing::debug;

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

/// Build a data frame for transmission
/// 
/// Frame format:
/// - Byte 0: 0xAA (start)
/// - Byte 1: 0xC0 | (is_extended << 5) | dlc
/// - Byte 2-3: ID (little endian)
/// - Byte 4..4+dlc: Data
/// - Byte 4+dlc: 0x55 (footer)
pub fn build_data_frame(
    frame_type: CanFrameType,
    id: impl Into<u32>,
    data: &[u8],
) -> Result<Vec<u8>, &'static str> {
    if data.len() > 8 {
        return Err("Data too long (max 8 bytes)");
    }

    let id = id.into();
    let mut buffer = Vec::with_capacity(5 + data.len());

    // Start byte
    buffer.push(FRAME_START);

    // Info byte: 0xC0 | extended_flag | dlc
    let mut info: u8 = 0xC0;
    if frame_type == CanFrameType::Extended {
        info |= 0x20;
    }
    info |= data.len() as u8;
    buffer.push(info);

    // ID (little endian)
    buffer.push((id & 0xFF) as u8);
    buffer.push(((id >> 8) & 0xFF) as u8);

    // Data
    buffer.extend_from_slice(data);

    // Footer
    buffer.push(FRAME_FOOTER);

    Ok(buffer)
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

/// Parse incoming buffer and extract complete frames
/// 
/// Returns the number of bytes consumed from the buffer
pub fn parse_frames(
    buffer: &[u8],
    output: &mut Vec<ParsedFrame>,
    debug_traffic: bool,
) -> usize {
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
        let data: Vec<u8> = buffer[data_start..data_start + dlc].to_vec();

        // Determine frame type
        let is_extended = (buffer[offset + 1] & 0x20) != 0;

        if debug_traffic {
            debug!(
                "Parsed frame: id=0x{:03x}, extended={}, dlc={}, data={:02x?}",
                id, is_extended, dlc, data
            );
        }

        output.push(ParsedFrame {
            id,
            data,
            is_extended,
        });

        offset += frame_len;
    }

    offset
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
pub fn parse_can_id(hex_id: &str) -> Option<crate::CanId> {
    let hex: String = hex_id.chars().filter(|c| c.is_ascii_hexdigit()).collect();

    // Standard IDs are 11-bit (max 0x7FF), Extended IDs are 29-bit
    let id = u32::from_str_radix(&hex, 16).ok()?;

    if id <= 0x7FF {
        Some(crate::CanId::Std(id as u16))
    } else if id <= 0x1FFFFFFF {
        Some(crate::CanId::Extended(id))
    } else {
        None // ID too large for 29-bit extended
    }
}
