//! Concrete CAN frame type implementing [`embedded_can::Frame`].
//!
//! `embedded-can` only defines traits (`Frame`, `Id`, ...), so this crate
//! provides exactly one owned frame type, used for received messages.
//! Anything implementing [`embedded_can::Frame`] can be sent through the
//! frontends.

use embedded_can::{ExtendedId, Frame, Id, StandardId};

/// A CAN 2.0 message (classic CAN, up to 8 data bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanMessage {
    id: Id,
    data: [u8; 8],
    dlc: u8,
    rtr: bool,
}

impl CanMessage {
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
}

impl Frame for CanMessage {
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
        if self.rtr {
            &[]
        } else {
            &self.data[..self.dlc as usize]
        }
    }
}
