//! Demo of the `embedded-io` backend (sync + async) without real hardware,
//! using an in-memory loopback mock as the byte stream.

use std::collections::VecDeque;
use std::convert::Infallible;
use usb_can_a::backends::embedded_io::{AsyncCanUsbClient, CanUsbClient};
use usb_can_a::{CanSpeed, CanUsbConfig, Frame, StandardId};

/// In-memory loopback stream: written bytes can be read back.
#[derive(Default)]
struct LoopbackIo {
    buffer: VecDeque<u8>,
}

impl embedded_io::ErrorType for LoopbackIo {
    type Error = Infallible;
}

impl embedded_io::Read for LoopbackIo {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        let n = buf.len().min(self.buffer.len());
        for slot in &mut buf[..n] {
            *slot = self.buffer.pop_front().unwrap();
        }
        Ok(n)
    }
}

impl embedded_io::Write for LoopbackIo {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.buffer.extend(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl embedded_io_async::Read for LoopbackIo {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        embedded_io::Read::read(self, buf)
    }
}

impl embedded_io_async::Write for LoopbackIo {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        embedded_io::Write::write(self, buf)
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Minimal block_on for the always-ready mock (std-only, no runtime needed).
fn block_on<F: core::future::Future>(fut: F) -> F::Output {
    use std::pin::pin;
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    fn clone(_: *const ()) -> RawWaker {
        RawWaker::new(std::ptr::null(), &VTABLE)
    }
    fn noop(_: *const ()) {}
    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, noop, noop, noop);

    let waker = unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) };
    let mut cx = Context::from_waker(&waker);
    let mut fut = pin!(fut);
    loop {
        match fut.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return v,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();

    let config = CanUsbConfig {
        can_speed: CanSpeed::Bps500000,
        ..Default::default()
    };

    // --- Sync client ---
    // The mock loops bytes back, so anything we write can be read again.
    let mut client = CanUsbClient::new(LoopbackIo::default(), config.clone()).unwrap();
    client.io_mut().buffer.clear(); // drop the looped-back settings frame

    let frame = Frame::standard(StandardId::new(0x123).unwrap(), &[0x11, 0x22]);
    client.write_frame(&frame).unwrap();
    let msg = client.read().unwrap();
    println!("sync read back: id=0x{:03x} data={:02x?}", msg.raw_id(), msg.data());

    // --- Async client (same shape, async traits) ---
    block_on(async {
        let mut client = AsyncCanUsbClient::new(LoopbackIo::default(), config).await.unwrap();
        client.io_mut().buffer.clear();
        client.write_frame(&frame).await.unwrap();
        let msg = client.read().await.unwrap();
        println!("async read back: id=0x{:03x} data={:02x?}", msg.raw_id(), msg.data());
    });
}
