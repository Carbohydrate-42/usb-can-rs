pub mod wareshare_usb_can_a;

use crate::types::CanFrameType;
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
    
    fn build_settings_frame(
        &self,
        config: &Self::Config,
        out: &mut [u8],
    ) -> Result<usize, &'static str>;

    fn build_data_frame(
        &self,
        frame_type: CanFrameType,
        id: u32,
        data: &[u8],
        out: &mut [u8],
    ) -> Result<usize, &'static str>;
    
    fn parse_next_frame(
        &self,
        buffer: &[u8],
        data_out: &mut [u8; 8],
    ) -> (usize, Option<ParsedFrameMeta>);
    
    fn parse_frames(
        &self,
        buffer: &[u8],
        output: &mut Vec<ParsedFrame>,
    ) -> usize {
        let mut total_consumed = 0;
        let mut data_out = [0u8; 8];

        loop {
            let (consumed, frame) =
                self.parse_next_frame(&buffer[total_consumed..], &mut data_out);
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