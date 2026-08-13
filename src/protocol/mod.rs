pub mod wareshare_usb_can_a;

use crate::message::id_from_raw;
use crate::types::CanFrameType;
use alloc::string::String;
use alloc::vec::Vec;

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
/// Frame data is copied into the caller-provided buffer by
/// [`Protocol::parse_next_frame`].
#[derive(Debug, Clone, Copy)]
pub struct ParsedFrameMeta {
    /// Frame ID (11-bit or 29-bit)
    pub id: u16,
    /// Data Length Code (0-8)
    pub dlc: u8,
    /// True if extended frame (29-bit ID)
    pub is_extended: bool,
}

/// Wire protocol of a CAN adapter.
///
/// Implementations are typically zero-sized types; all state lives in the
/// associated [`Protocol::Config`].
pub trait Protocol {
    /// Protocol-specific configuration (CAN speed, mode, filters, ...)
    type Config;

    /// Maximum size in bytes of the settings frame written by
    /// [`Protocol::build_settings_frame`]
    const SETTINGS_FRAME_MAX_SIZE: usize;
    /// Maximum size in bytes of a data frame written by
    /// [`Protocol::build_data_frame`]
    const DATA_FRAME_MAX_SIZE: usize;

    /// Build the settings frame sent right after connecting.
    ///
    /// Returns the number of bytes written to `out`.
    fn build_settings_frame(
        &self,
        config: &Self::Config,
        out: &mut [u8],
    ) -> Result<usize, &'static str>;

    /// Build a wire data frame for the transmission of a CAN message.
    ///
    /// Returns the number of bytes written to `out`.
    fn build_data_frame(
        &self,
        frame_type: CanFrameType,
        id: u32,
        data: &[u8],
        out: &mut [u8],
    ) -> Result<usize, &'static str>;

    /// Scan `buffer` for the next complete incoming frame, skipping junk bytes.
    ///
    /// Returns `(consumed, frame)`:
    /// - `consumed` is the number of bytes that can be dropped from the front
    ///   of the buffer (junk bytes, plus the frame itself if one was found).
    ///   When an incomplete frame is found, only the junk before it is
    ///   reported as consumed so the partial frame stays in the buffer.
    /// - `frame` is `Some(meta)` when a complete frame was parsed; its data
    ///   (0-8 bytes) is copied into `data_out`.
    fn parse_next_frame(
        &self,
        buffer: &[u8],
        data_out: &mut [u8; 8],
        debug_traffic: bool,
    ) -> (usize, Option<ParsedFrameMeta>);

    /// Parse all complete frames in `buffer`, appending them to `output`.
    ///
    /// Allocating convenience built on [`Protocol::parse_next_frame`].
    /// Returns the number of bytes consumed from the buffer.
    fn parse_frames(
        &self,
        buffer: &[u8],
        output: &mut Vec<ParsedFrame>,
        debug_traffic: bool,
    ) -> usize {
        let mut total_consumed = 0;
        let mut data_out = [0u8; 8];

        loop {
            let (consumed, frame) =
                self.parse_next_frame(&buffer[total_consumed..], &mut data_out, debug_traffic);
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
