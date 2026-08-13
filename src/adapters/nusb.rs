//! TODO: USB CAN-FD adapter over `nusb` (direct USB, no serial emulation).
//!
//! Not wired into the build yet (no module declaration, no feature flag).
//!
//! Plan:
//! - [ ] protocol: implement the device's wire protocol under `src/protocol/`
//!       (`Protocol` impl: settings frame, data frame build/parse; FD data
//!       frames up to 64 bytes — check whether the `Protocol` trait's
//!       `[u8; 8]` data buffer needs generalizing)
//! - [ ] transport: nusb bulk endpoint I/O behind the shared `Transport`
//!       pattern (see `crate::adapters::tokio_serial`)
//! - [ ] adapter: expose the same client/split style API as `tokio_serial`
//! - [ ] frame type: use `embedded_can::fd::Frame` for CAN-FD (verify exact
//!       trait shape on docs.rs before starting)
//! - [ ] Cargo.toml: add `nusb` dependency + `nusb` feature flag, then
//!       declare this module in `adapters/mod.rs` behind that flag
