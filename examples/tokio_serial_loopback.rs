//! Loopback test: the adapter is put into `CanMode::Loopback`, so every frame
//! it sends is received back by itself. TX and RX paths can be verified on a
//! single device, without a CAN bus partner.
//!
//! Requires a real adapter on COM4.

use std::time::Duration;
use tokio_serial::SerialPortBuilderExt;
use usb_can::adapters::tokio_serial::CanUsbClient;
use usb_can::protocol::wareshare_usb_can_a;
use usb_can::{CanMessage, Frame, StandardId};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();

    // The serial port is opened by the caller
    let serial = tokio_serial::new("COM4", 2_000_000)
        .data_bits(tokio_serial::DataBits::Eight)
        .stop_bits(tokio_serial::StopBits::Two)
        .parity(tokio_serial::Parity::None)
        .open_native_async()?;

    // Loopback mode: the adapter receives its own transmissions
    let config = wareshare_usb_can_a::Config {
        can_speed: wareshare_usb_can_a::CanSpeed::Bps1000000,
        can_mode: wareshare_usb_can_a::CanMode::Loopback,
        ..Default::default()
    };

    let mut client =
        CanUsbClient::new(serial, wareshare_usb_can_a::WaveshareUsbCanA, config).await?;

    let req = CanMessage::new(StandardId::new(0x123).unwrap(), &[0xDE, 0xAD, 0xBE, 0xEF]).unwrap();
    client.write(&req).await?;
    println!("Sent:     {:?}", req);

    // The looped-back frame should come right back
    let resp = client.read(Duration::from_secs(1)).await?;
    println!("Loopback: {:?}", resp);

    assert_eq!(resp.id(), req.id());
    assert_eq!(resp.data(), req.data());
    println!("Loopback OK");

    Ok(())
}
