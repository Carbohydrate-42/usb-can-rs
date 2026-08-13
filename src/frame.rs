//! CAN Frame types with multiple constructors

use crate::message::{id_from_raw, CanMessage};
use crate::types::CanFrameType;
use embedded_can::{ExtendedId, Id, StandardId};

/// A CAN frame that can be sent over USB-CAN adapter
///
/// This is a builder-style wrapper around [`CanMessage`] with additional
/// constructors for common use cases.
#[derive(Debug, Clone)]
pub struct Frame {
    /// The underlying CAN message
    pub message: CanMessage,
    /// Frame type (Standard or Extended)
    pub frame_type: CanFrameType,
}

impl Frame {
    /// Create a frame from a [`CanMessage`]
    pub fn from_message(message: CanMessage) -> Self {
        let frame_type = if embedded_can::Frame::is_extended(&message) {
            CanFrameType::Extended
        } else {
            CanFrameType::Standard
        };
        Self {
            message,
            frame_type,
        }
    }

    /// Create a standard frame with ID and data
    ///
    /// # Example
    /// ```
    /// use usb_can::{Frame, StandardId};
    ///
    /// let frame = Frame::standard(StandardId::new(0x123).unwrap(), &[0x11, 0x22, 0x33, 0x44]);
    /// ```
    pub fn standard(id: StandardId, data: &[u8]) -> Self {
        Self {
            message: CanMessage::new(id, data).expect("data must be at most 8 bytes"),
            frame_type: CanFrameType::Standard,
        }
    }

    /// Create an extended frame with ID and data
    pub fn extended(id: ExtendedId, data: &[u8]) -> Self {
        Self {
            message: CanMessage::new(id, data).expect("data must be at most 8 bytes"),
            frame_type: CanFrameType::Extended,
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

    /// Create a frame from raw ID value and data
    ///
    /// ID is automatically determined as Standard (<= 0x7FF) or Extended
    pub fn with_id(id: u32, data: &[u8]) -> Self {
        let can_id = id_from_raw(id).expect("ID must fit in 29 bits");
        let frame_type = if id <= 0x7FF {
            CanFrameType::Standard
        } else {
            CanFrameType::Extended
        };
        Self {
            message: CanMessage::new(can_id, data).expect("data must be at most 8 bytes"),
            frame_type,
        }
    }

    /// Create a frame from hex strings
    ///
    /// # Example
    /// ```
    /// use usb_can::Frame;
    ///
    /// let frame = Frame::from_hex("123", "DEADBEEF").unwrap();
    /// ```
    pub fn from_hex(hex_id: &str, hex_data: &str) -> Option<Self> {
        let hex: alloc::string::String = hex_id.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        let id = u32::from_str_radix(&hex, 16).ok()?;
        let data = Self::hex_to_bytes(hex_data)?;
        Some(Self::with_id(id, &data))
    }

    /// Create a remote transmission request (RTR) frame
    pub fn rtr(id: impl Into<Id>) -> Self {
        let id = id.into();
        let frame_type = match id {
            Id::Standard(_) => CanFrameType::Standard,
            Id::Extended(_) => CanFrameType::Extended,
        };
        Self {
            message: CanMessage::new_rtr(id, 0).expect("dlc 0 is always valid"),
            frame_type,
        }
    }

    /// Get the frame ID
    pub fn id(&self) -> Id {
        self.message.id()
    }

    /// Get the raw ID value as `u32`
    pub fn raw_id(&self) -> u32 {
        self.message.raw_id()
    }

    /// Get the frame data
    pub fn data(&self) -> &[u8] {
        self.message.data()
    }

    /// Get the DLC (Data Length Code)
    pub fn dlc(&self) -> u8 {
        self.message.dlc()
    }

    /// Check if this is an extended frame
    pub fn is_extended(&self) -> bool {
        self.frame_type == CanFrameType::Extended
    }

    /// Check if this is an RTR frame
    pub fn is_rtr(&self) -> bool {
        self.message.is_rtr()
    }
}

impl From<CanMessage> for Frame {
    fn from(message: CanMessage) -> Self {
        Self::from_message(message)
    }
}

impl From<Frame> for CanMessage {
    fn from(frame: Frame) -> Self {
        frame.message
    }
}
