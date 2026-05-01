//! CAN Frame types with multiple constructors

use crate::protocol::hex_to_bytes;
use crate::types::CanFrameType;
use zencan_common::{CanId, CanMessage};

/// A CAN frame that can be sent over USB-CAN adapter
/// 
/// This is a builder-style wrapper around CanMessage with additional
/// constructors for common use cases.
#[derive(Debug, Clone)]
pub struct Frame {
    /// The underlying CAN message
    pub message: CanMessage,
    /// Frame type (Standard or Extended)
    pub frame_type: CanFrameType,
}

impl Frame {
    /// Create a frame from a CanMessage
    pub fn from_message(message: CanMessage) -> Self {
        let frame_type = if message.id.is_extended() {
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
    /// use usb_can_a::{Frame, CanId};
    /// 
    /// let frame = Frame::standard(CanId::Std(0x123), &[0x11, 0x22, 0x33, 0x44]);
    /// ```
    pub fn standard(id: CanId, data: &[u8]) -> Self {
        Self {
            message: CanMessage::new(id, data),
            frame_type: CanFrameType::Standard,
        }
    }

    /// Create an extended frame with ID and data
    pub fn extended(id: CanId, data: &[u8]) -> Self {
        Self {
            message: CanMessage::new(id, data),
            frame_type: CanFrameType::Extended,
        }
    }

    /// Create a frame from raw ID value and data
    /// 
    /// ID is automatically determined as Standard (<= 0x7FF) or Extended
    pub fn with_id(id: u32, data: &[u8]) -> Self {
        let (can_id, frame_type) = if id <= 0x7FF {
            (CanId::Std(id as u16), CanFrameType::Standard)
        } else {
            (CanId::Extended(id), CanFrameType::Extended)
        };
        Self {
            message: CanMessage::new(can_id, data),
            frame_type,
        }
    }

    /// Create a frame from hex strings
    /// 
    /// # Example
    /// ```
    /// use usb_can_a::Frame;
    /// 
    /// let frame = Frame::from_hex("123", "DEADBEEF").unwrap();
    /// ```
    pub fn from_hex(hex_id: &str, hex_data: &str) -> Option<Self> {
        let hex: String = hex_id.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        let id = u32::from_str_radix(&hex, 16).ok()?;
        let data = hex_to_bytes(hex_data)?;
        Some(Self::with_id(id, &data))
    }

    /// Create a remote transmission request (RTR) frame
    pub fn rtr(id: CanId) -> Self {
        Self {
            message: CanMessage::new_rtr(id),
            frame_type: CanFrameType::Standard,
        }
    }

    /// Get the frame ID
    pub fn id(&self) -> CanId {
        self.message.id
    }

    /// Get the frame data
    pub fn data(&self) -> &[u8] {
        self.message.data()
    }

    /// Get the DLC (Data Length Code)
    pub fn dlc(&self) -> u8 {
        self.message.dlc
    }

    /// Check if this is an extended frame
    pub fn is_extended(&self) -> bool {
        self.frame_type == CanFrameType::Extended
    }

    /// Check if this is an RTR frame
    pub fn is_rtr(&self) -> bool {
        self.message.rtr
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
