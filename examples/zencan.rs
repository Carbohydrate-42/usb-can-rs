//! Demo of the `zencan` frontend: split the adapter into a sender/receiver
//! pair that can be handed to zencan's `BusManager`.

use usb_can_a::backends::tokio_serial::TokioSerialConfig;
use usb_can_a::frontends::zencan::split_for_zencan;
use usb_can_a::{CanFrameType, CanSpeed, CanUsbConfig};
use zencan_common::traits::{AsyncCanReceiver, AsyncCanSender};
use zencan_common::{CanId, CanMessage};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();

	let config = TokioSerialConfig {
		device: "COM4".to_string(),
		baudrate: 2_000_000,
		can: CanUsbConfig {
			can_speed: CanSpeed::Bps1000000,
			frame_type: CanFrameType::Standard,
			..Default::default()
		},
	};

	// (sender, receiver) compatible with zencan's BusManager::new
	let (mut tx, mut rx) = split_for_zencan(config).await?;

	// Send via the zencan AsyncCanSender trait
	let msg = CanMessage::new(CanId::Std(0x123), &[0x11, 0x22]);
	tx.send(msg).await?;

	// Receive via the zencan AsyncCanReceiver trait
	while let Ok(msg) = rx.recv().await {
		println!("Received: {:?}", msg);
	}

	Ok(())
}
