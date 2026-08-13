pub mod wareshare_usb_can_a;

use embedded_can::Frame;

/// Metadata of a single parsed frame.
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

    /// Build the wire data frame for `frame` into `out`.
    ///
    /// The standard/extended distinction is derived from the frame's
    /// [`embedded_can::Id`]. Returns the number of bytes written.
    fn build_data_frame(
        &self,
        frame: &impl Frame,
        out: &mut [u8],
    ) -> Result<usize, &'static str>;
    
    /// Scan `buffer` for the next complete frame.
    ///
    /// Returns `(consumed, frame)`; see the implementation for details.
    /// Call in a loop to drain a buffer.
    fn parse_next_frame(
        &self,
        buffer: &[u8],
        data_out: &mut [u8; 8],
    ) -> (usize, Option<ParsedFrameMeta>);
}