//! Demo of the `zencan` frontend: split the adapter into a sender/receiver
//! pair that can be handed to zencan's `BusManager`.

use tokio_serial::SerialPortBuilderExt;
use usb_can::frontend::zencan_tokio_serial::ZenCanSender;
use usb_can::protocol::wareshare_usb_can_a;
use zencan_common::traits::{AsyncCanReceiver, AsyncCanSender};
use zencan_common::{CanId, CanMessage};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();

    // The serial port is opened by the caller
    let serial = tokio_serial::new("COM4", 2_000_000)
        .data_bits(tokio_serial::DataBits::Eight)
        .stop_bits(tokio_serial::StopBits::Two)
        .parity(tokio_serial::Parity::None)
        .open_native_async()?;

    let config = wareshare_usb_can_a::Config {
        can_speed: wareshare_usb_can_a::CanSpeed::Bps1000000,
        ..Default::default()
    };

    // (sender, receiver) compatible with zencan's BusManager::new
    let (mut tx, mut rx) =
        ZenCanSender::split(serial, wareshare_usb_can_a::WaveshareUsbCanA, &config).await?;

    // Send via the zencan AsyncCanSender trait
    let msg = CanMessage::new(CanId::Std(0x123), &[0x11, 0x22]);
    tx.send(msg).await?;

    // Receive via the zencan AsyncCanReceiver trait
    while let Ok(msg) = rx.recv().await {
        println!("Received: {:?}", msg);
    }

    Ok(())
}
