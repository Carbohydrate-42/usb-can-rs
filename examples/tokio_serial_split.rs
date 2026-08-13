use tokio_serial::SerialPortBuilderExt;
use usb_can::frontend::tokio_serial::CanUsbSender;
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

    let config = wareshare_usb_can_a::Config {
        can_speed: wareshare_usb_can_a::CanSpeed::Bps1000000,
        // filter_id: 0x100,
        // mask_id: 0x7F0,
        ..Default::default()
    };

    // Returns (sender, receiver) - same style as mpsc::channel!
    let (tx, mut rx) = CanUsbSender::split(
        serial,
        wareshare_usb_can_a::WaveshareUsbCanA,
        &config,
    )
    .await?;

    // tx is Clone-able, supporting multiple producers
    let tx2 = tx.clone();

    // Producer task 1
    tokio::spawn(async move {
        let frame = CanMessage::new(StandardId::new(0x123).unwrap(), &[0x11]).unwrap();
        tx.send(&frame).await.unwrap();
    });

    // Producer task 2
    tokio::spawn(async move {
        let frame = CanMessage::new(StandardId::new(0x456).unwrap(), &[0x22]).unwrap();
        tx2.send(&frame).await.unwrap();
    });

    // Consumer
    while let Some(msg) = rx.recv().await {
        println!("Received: {:?}", msg);
    }

    Ok(())
}
