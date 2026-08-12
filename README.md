# USB-CAN-A Rust Library

A Rust library for USB-CAN adapter communication.

Core protocol is `no_std` compatible. CAN message types are based on
[`embedded-can`](https://docs.rs/embedded-can) (`Id`, `StandardId`, `ExtendedId`,
`Frame` trait), with this crate's own `CanMessage` implementing it.

# Features

- `std` (default): enables `std` support
- `log` / `defmt`: logging backend — pick exactly one (mutually exclusive);
  with neither, all log statements compile to no-ops
- `tokio-serial`: std adapter, async transport over `tokio-serial`
  (module `tokio_serial`: `split`, `client`, `CanUsbClient`, ...)
- `embedded-io`: no_std adapter over `embedded-io` / `embedded-io-async`
  (module `embedded_io`: `CanUsbClient` sync + `AsyncCanUsbClient`)
- `zencan`: extension adaptor for zencan's `BusManager`
  (module `zencan`: `split_for_zencan`, `ZenCanSender`, `ZenCanReceiver`)

# how to use

```toml
# std + tokio-serial transport
usb-can-a-rs = { git = "https://github.com/Carbohydrate-42/usb-can-a-rs", features = ["tokio-serial", "log"] }

# no_std + embedded-io transport
usb-can-a-rs = { git = "https://github.com/Carbohydrate-42/usb-can-a-rs", default-features = false, features = ["embedded-io", "defmt"] }

# zencan adaptor
usb-can-a-rs = { git = "https://github.com/Carbohydrate-42/usb-can-a-rs", features = ["zencan", "log"] }
```
