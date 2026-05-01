pub mod frame;
pub mod protocol;
pub mod types;
pub mod client_with_split;
pub mod split_for_zencan;

// Re-export from zencan-common
pub use zencan_common::{CanId, CanMessage};

// Re-export our modules
pub use client_with_split::{CanUsbClient, ClientError, client, split};
pub use frame::Frame;
pub use protocol::{hex_to_bytes, parse_can_id};
pub use types::{CanFrameType, CanMode, CanSpeed, CanUsbConfig, InvalidCanSpeed, PayloadMode};
