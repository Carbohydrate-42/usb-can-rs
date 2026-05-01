use std::time::Duration;
use tracing_subscriber::EnvFilter;
use usb_can_a::{client, CanSpeed, CanUsbConfig, Frame};
use zencan_common::CanId;

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

	// 独占式客户端
	let mut client = client(config).await?;

	// 请求-响应模式
	let req = Frame::standard(CanId::Std(0x123), &[0x01]);
	client.write(&req).await?;

	// 阻塞等待响应（独占读取）
	let resp = client.read(Duration::from_secs(1)).await?;
	println!("Response: {:?}", resp);
	

	Ok(())
}
