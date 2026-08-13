use std::time::Duration;
use usb_can_a::backends::tokio_serial::{client, TokioSerialConfig};
use usb_can_a::{CanFrameType, CanSpeed, CanUsbConfig, Frame, StandardId};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();

	let config = TokioSerialConfig {
		device: "COM4".to_string(),
		baudrate: 2_000_000,
		can: CanUsbConfig {
			can_speed: CanSpeed::Bps1000000,
			frame_type: CanFrameType::Standard,
			// filter_id: 0x100,
			// mask_id: 0x7F0,
			..Default::default()
		},
	};

	// Exclusive client
	let mut client = client(config).await?;

	// Request-response pattern
	let req = Frame::standard(StandardId::new(0x123).unwrap(), &[0x01]);
	client.write(&req).await?;

	// Block waiting for the response (exclusive read)
	let resp = client.read(Duration::from_secs(1)).await?;
	println!("Response: {:?}", resp);


	Ok(())
}
