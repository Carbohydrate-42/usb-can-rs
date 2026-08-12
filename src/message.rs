//! CAN message type built on `embedded-can` traits.

use embedded_can::{ExtendedId, Frame as CanFrame, Id, StandardId};

/// A CAN 2.0 message (classic CAN, up to 8 data bytes).
///
/// Implements [`embedded_can::Frame`], so it can be used with any
/// `embedded-can` based ecosystem crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanMessage {
    id: Id,
    data: [u8; 8],
    dlc: u8,
    rtr: bool,
}

impl CanMessage {
    /// Create a new data message.
    ///
    /// Returns `None` if `data` is longer than 8 bytes.
    pub fn new(id: impl Into<Id>, data: &[u8]) -> Option<Self> {
        <Self as CanFrame>::new(id, data)
    }

    /// Create a new remote transmission request (RTR) message.
    ///
    /// Returns `None` if `dlc` is greater than 8.
    pub fn new_rtr(id: impl Into<Id>, dlc: u8) -> Option<Self> {
        <Self as CanFrame>::new_remote(id, dlc as usize)
    }

    /// The message ID.
    pub fn id(&self) -> Id {
        self.id
    }

    /// The message data (empty for RTR frames).
    pub fn data(&self) -> &[u8] {
        <Self as CanFrame>::data(self)
    }

    /// Data Length Code.
    pub fn dlc(&self) -> u8 {
        self.dlc
    }

    /// True if this is a remote transmission request.
    pub fn is_rtr(&self) -> bool {
        self.rtr
    }

    /// Raw 11/29-bit ID value as `u32`.
    pub fn raw_id(&self) -> u32 {
        raw_id(self.id)
    }
}

impl CanFrame for CanMessage {
    fn new(id: impl Into<Id>, data: &[u8]) -> Option<Self> {
        if data.len() > 8 {
            return None;
        }
        let mut buf = [0u8; 8];
        buf[..data.len()].copy_from_slice(data);
        Some(Self {
            id: id.into(),
            data: buf,
            dlc: data.len() as u8,
            rtr: false,
        })
    }

    fn new_remote(id: impl Into<Id>, dlc: usize) -> Option<Self> {
        if dlc > 8 {
            return None;
        }
        Some(Self {
            id: id.into(),
            data: [0u8; 8],
            dlc: dlc as u8,
            rtr: true,
        })
    }

    fn is_extended(&self) -> bool {
        matches!(self.id, Id::Extended(_))
    }

    fn is_remote_frame(&self) -> bool {
        self.rtr
    }

    fn id(&self) -> Id {
        self.id
    }

    fn dlc(&self) -> usize {
        self.dlc as usize
    }

    fn data(&self) -> &[u8] {
        if self.rtr { &[] } else { &self.data[..self.dlc as usize] }
    }
}

/// Extract the raw ID value (11-bit or 29-bit) as `u32`.
pub fn raw_id(id: Id) -> u32 {
    match id {
        Id::Standard(id) => id.as_raw() as u32,
        Id::Extended(id) => id.as_raw(),
    }
}

/// Build an [`Id`] from a raw value, choosing standard vs. extended by range.
///
/// Returns `None` if `raw` does not fit in 29 bits.
pub fn id_from_raw(raw: u32) -> Option<Id> {
    if raw <= 0x7FF {
        StandardId::new(raw as u16).map(Id::Standard)
    } else {
        ExtendedId::new(raw).map(Id::Extended)
    }
}
