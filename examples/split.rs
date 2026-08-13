use tokio_serial::SerialPortBuilderExt;
use usb_can::backends::tokio_serial::split;
use usb_can::{CanFrameType, CanSpeed, Frame, StandardId, WaveshareUsbCanA, WaveshareUsbCanAConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();

	// The serial port is opened by the caller
	let serial = tokio_serial::new("COM4", 2_000_000)
		.data_bits(tokio_serial::DataBits::Eight)
		.stop_bits(tokio_serial::StopBits::Two)
		.parity(tokio_serial::Parity::None)
		.open_native_async()?;

	let config = WaveshareUsbCanAConfig {
		can_speed: CanSpeed::Bps1000000,
		frame_type: CanFrameType::Standard,
		// filter_id: 0x100,
		// mask_id: 0x7F0,
		..Default::default()
	};

	// Returns (sender, receiver) - same style as mpsc::channel!
	let (tx, mut rx) = split(serial, WaveshareUsbCanA, &config, false).await?;

	// tx is Clone-able, supporting multiple producers
	let tx2 = tx.clone();

	// Producer task 1
	tokio::spawn(async move {
		let frame = Frame::standard(StandardId::new(0x123).unwrap(), &[0x11]);
		tx.send(frame).await.unwrap();
	});

	// Producer task 2
	tokio::spawn(async move {
		let frame = Frame::standard(StandardId::new(0x456).unwrap(), &[0x22]);
		tx2.send(frame).await.unwrap();
	});

	// Consumer
	while let Some(msg) = rx.recv().await {
		println!("Received: {:?}", msg);
	}

	Ok(())
}
