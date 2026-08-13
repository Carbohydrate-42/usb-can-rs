# USB-CAN Rust Library

A Rust library for USB-CAN adapter communication.

Core is `no_std` compatible and allocation-free. CAN frame handling is based
on [`embedded-can`](https://docs.rs/embedded-can): send APIs accept any
`embedded_can::Frame` implementation, and received frames are returned as
this crate's `CanMessage`, which implements the `Frame` trait.

The wire protocol is abstracted behind the `protocol::Protocol` trait — the
USB-CAN-A binary protocol (`protocol::wareshare_usb_can_a::WaveshareUsbCanA`)
is one implementation of it.

# Layout

- `protocol` — wire protocol abstraction:
  - `protocol::Protocol`: the trait (settings frame, data frame build/parse)
  - `protocol::wareshare_usb_can_a::WaveshareUsbCanA`: USB-CAN-A
    implementation + its config (`Config`, `CanSpeed`, `CanMode`)
- `frontend` — transports/adaptors that move frames to/from the adapter:
  - `frontend::tokio_serial` (feature `tokio-serial`, std): async serial port
    transport over a caller-opened `SerialStream` (`split`, `client`,
    `CanUsbSender`, `CanUsbClient`)
  - `frontend::zencan` (feature `zencan`): adaptor for zencan's
    `BusManager` (`split_for_zencan`, `ZenCanSender`, `ZenCanReceiver`)

# Features

- `std` (default): enables `std` support
- `log` / `defmt`: logging backend — pick exactly one (mutually exclusive);
  with neither, all log statements compile to no-ops
- `tokio-serial`, `zencan`: see layout above

# how to use

```toml
# std + tokio-serial transport
usb-can-a-rs = { git = "https://github.com/Carbohydrate-42/usb-can-rs", features = ["tokio-serial", "log"] }

# zencan frontend
usb-can-a-rs = { git = "https://github.com/Carbohydrate-42/usb-can-rs", features = ["zencan", "log"] }
```

```rust
use usb_can::protocol::wareshare_usb_can_a::{Config, WaveshareUsbCanA};

// The serial port is opened by the caller; protocol is chosen explicitly
let serial = tokio_serial::new("COM4", 2_000_000).open_native_async()?;
let (tx, rx) = usb_can::frontend::tokio_serial::split(
    serial, WaveshareUsbCanA, &Config::default(), false,
).await?;
```

# examples

```sh
# Require a real adapter on COM4
cargo run --example tokio_serial_client --features tokio-serial,log
cargo run --example tokio_serial_split --features tokio-serial,log
cargo run --example zencan --features zencan,log
```
