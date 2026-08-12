use usb_can_a::tokio_serial::{split, TokioSerialConfig};
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

	// 返回 (sender, receiver) - 和 mpsc::channel 一样的风格！
	let (tx, mut rx) = split(config).await?;

	// tx 可以 Clone，多生产者
	let tx2 = tx.clone();

	// 生产者任务 1
	tokio::spawn(async move {
		let frame = Frame::standard(StandardId::new(0x123).unwrap(), &[0x11]);
		tx.send(frame).await.unwrap();
	});

	// 生产者任务 2
	tokio::spawn(async move {
		let frame = Frame::standard(StandardId::new(0x456).unwrap(), &[0x22]);
		tx2.send(frame).await.unwrap();
	});

	// 消费者
	while let Some(msg) = rx.recv().await {
		println!("Received: {:?}", msg);
	}

	Ok(())
}
