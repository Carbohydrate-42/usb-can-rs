use std::time::Duration;
use tokio_serial::SerialPortBuilderExt;
use usb_can_a::backends::tokio_serial::client;
use usb_can_a::{CanFrameType, CanSpeed, Frame, StandardId, UsbCanA, UsbCanAConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();

	// The serial port is opened by the caller
	let serial = tokio_serial::new("COM4", 2_000_000)
		.data_bits(tokio_serial::DataBits::Eight)
		.stop_bits(tokio_serial::StopBits::Two)
		.parity(tokio_serial::Parity::None)
		.open_native_async()?;

	let config = UsbCanAConfig {
		can_speed: CanSpeed::Bps1000000,
		frame_type: CanFrameType::Standard,
		// filter_id: 0x100,
		// mask_id: 0x7F0,
		..Default::default()
	};

	// Exclusive client
	let mut client = client(serial, UsbCanA, config, false).await?;

	// Request-response pattern
	let req = Frame::standard(StandardId::new(0x123).unwrap(), &[0x01]);
	client.write(&req).await?;

	// Block waiting for the response (exclusive read)
	let resp = client.read(Duration::from_secs(1)).await?;
	println!("Response: {:?}", resp);


	Ok(())
}
