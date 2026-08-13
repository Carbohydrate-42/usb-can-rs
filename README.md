# USB-CAN-A Rust Library

A Rust library for USB-CAN adapter communication.

Core protocol is `no_std` compatible. CAN message types are based on
[`embedded-can`](https://docs.rs/embedded-can) (`Id`, `StandardId`, `ExtendedId`,
`Frame` trait), with this crate's own `CanMessage` implementing it.

# Layout

- `backends` — transports that move bytes to/from the adapter:
  - `backends::tokio_serial` (feature `tokio-serial`, std): async serial port
    transport (`split`, `client`, `CanUsbClient`, ...)
  - `backends::embedded_io` (feature `embedded-io`, no_std): sync
    (`CanUsbClient`) + async (`AsyncCanUsbClient`) transport over any
    `embedded-io` byte stream
- `frontends` — adaptors exposing foreign interfaces:
  - `frontends::zencan` (feature `zencan`): adaptor for zencan's
    `BusManager` (`split_for_zencan`, `ZenCanSender`, `ZenCanReceiver`)

# Features

- `std` (default): enables `std` support
- `log` / `defmt`: logging backend — pick exactly one (mutually exclusive);
  with neither, all log statements compile to no-ops
- `tokio-serial`, `embedded-io`, `zencan`: see layout above

# how to use

```toml
# std + tokio-serial backend
usb-can-a-rs = { git = "https://github.com/Carbohydrate-42/usb-can-a-rs", features = ["tokio-serial", "log"] }

# no_std + embedded-io backend
usb-can-a-rs = { git = "https://github.com/Carbohydrate-42/usb-can-a-rs", default-features = false, features = ["embedded-io", "defmt"] }

# zencan frontend
usb-can-a-rs = { git = "https://github.com/Carbohydrate-42/usb-can-a-rs", features = ["zencan", "log"] }
```

# examples

```sh
# Runs without hardware (in-memory loopback mock)
cargo run --example embedded_io_loopback --features embedded-io,log

# Require a real adapter on COM4
cargo run --example client --features tokio-serial,log
cargo run --example split --features tokio-serial,log
cargo run --example zencan --features zencan,log
```
