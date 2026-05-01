use tracing_subscriber::EnvFilter;
use usb_can_a::{CanUsbConfig, Frame, CanId, CanSpeed, split};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	tracing_subscriber::fmt()
		.with_env_filter(
			EnvFilter::try_from_default_env()
				.unwrap_or_else(|_| EnvFilter::new("debug")),
		)
		.init();

	let device = "COM4".to_string();

	let config = CanUsbConfig {
		device,
		baudrate: 2_000_000,
		can_speed: CanSpeed::Bps1000000,
		frame_type: usb_can_a::CanFrameType::Standard,
		// filter_id: 0x100,
		// mask_id: 0x7F0,
		..Default::default()
	};

	// 返回 (sender, receiver) - 和 mpsc::channel 一样的风格！
	let (tx, mut rx) = split(config).await?;

	// tx 可以 Clone，多生产者
	let tx2 = tx.clone();

	// 生产者任务 1
	tokio::spawn(async move {
		let frame = Frame::standard(CanId::Std(0x123), &[0x11]);
		tx.send(frame).await.unwrap();
	});

	// 生产者任务 2
	tokio::spawn(async move {
		let frame = Frame::standard(CanId::Std(0x456), &[0x22]);
		tx2.send(frame).await.unwrap();
	});

	// 消费者
	while let Some(msg) = rx.recv().await {
		println!("Received: {:?}", msg);
	}

	Ok(())
}
