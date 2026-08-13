//! Tests for the `embedded-io` backend using an in-memory mock stream.

use std::collections::VecDeque;
use std::convert::Infallible;
use usb_can::backends::embedded_io::{AsyncCanUsbClient, CanUsbClient};
use usb_can::protocol::wareshare_usb_can_a;
use usb_can::{ExtendedId, Frame, Id, StandardId};

/// In-memory stream: `incoming` is what the adapter "sent" us,
/// `written` collects everything the client writes out.
#[derive(Default)]
struct MockIo {
    incoming: VecDeque<u8>,
    written: Vec<u8>,
}

impl MockIo {
    fn feed(&mut self, bytes: &[u8]) {
        self.incoming.extend(bytes);
    }
}

impl embedded_io::ErrorType for MockIo {
    type Error = Infallible;
}

impl embedded_io::Read for MockIo {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        let n = buf.len().min(self.incoming.len());
        for slot in &mut buf[..n] {
            *slot = self.incoming.pop_front().unwrap();
        }
        Ok(n)
    }
}

impl embedded_io::Write for MockIo {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.written.extend(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl embedded_io_async::Read for MockIo {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        embedded_io::Read::read(self, buf)
    }
}

impl embedded_io_async::Write for MockIo {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        embedded_io::Write::write(self, buf)
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Minimal block_on for the always-ready mock.
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

fn config() -> wareshare_usb_can_a::Config {
    wareshare_usb_can_a::Config {
        can_speed: wareshare_usb_can_a::CanSpeed::Bps500000,
        ..Default::default()
    }
}

fn expected_settings() -> [u8; 20] {
    let c = config();
    wareshare_usb_can_a::WaveshareUsbCanA::build_settings_frame(
        c.can_speed as u8,
        c.can_mode as u8,
        c.frame_type as u8,
        c.filter_id,
        c.mask_id,
    )
}

#[test]
fn test_sync_new_sends_settings() {
    let client = CanUsbClient::new(MockIo::default(), wareshare_usb_can_a::WaveshareUsbCanA, config(), false).unwrap();
    assert_eq!(client.io().written, expected_settings());
}

#[test]
fn test_sync_write_frame() {
    let mut client = CanUsbClient::new(MockIo::default(), wareshare_usb_can_a::WaveshareUsbCanA, config(), false).unwrap();
    let frame = Frame::standard(StandardId::new(0x123).unwrap(), &[0x11, 0x22]);
    client.write_frame(&frame).unwrap();

    let mut expected = expected_settings().to_vec();
    expected.extend(wareshare_usb_can_a::WaveshareUsbCanA::build_data_frame(frame.frame_type, 0x123u32, &[0x11, 0x22]).unwrap());
    assert_eq!(client.io().written, expected);
}

#[test]
fn test_sync_poll_read_parses_frame() {
    let mut client = CanUsbClient::new(MockIo::default(), wareshare_usb_can_a::WaveshareUsbCanA, config(), false).unwrap();

    // Feed a wire frame: standard ID 0x123, data [0xDE, 0xAD]
    let wire = wareshare_usb_can_a::WaveshareUsbCanA::build_data_frame(
        usb_can::CanFrameType::Standard,
        0x123u32,
        &[0xDE, 0xAD],
    )
    .unwrap();
    client.io_mut().feed(&wire);

    let msg = client.poll_read().unwrap().expect("expected a message");
    assert_eq!(msg.id(), Id::Standard(StandardId::new(0x123).unwrap()));
    assert_eq!(msg.data(), &[0xDE, 0xAD]);
}

#[test]
fn test_sync_read_extended_frame() {
    let mut client = CanUsbClient::new(MockIo::default(), wareshare_usb_can_a::WaveshareUsbCanA, config(), false).unwrap();

    let wire =  wareshare_usb_can_a::WaveshareUsbCanA::build_data_frame(
        usb_can::CanFrameType::Extended,
        0x1234u32,
        &[0xAA],
    )
    .unwrap();
    client.io_mut().feed(&wire);

    let msg = client.read().unwrap();
    assert_eq!(msg.id(), Id::Extended(ExtendedId::new(0x1234).unwrap()));
    assert_eq!(msg.data(), &[0xAA]);
}

#[test]
fn test_sync_partial_frame_waits_for_more_data() {
    let mut client = CanUsbClient::new(MockIo::default(), wareshare_usb_can_a::WaveshareUsbCanA, config(), false).unwrap();

    let wire =  wareshare_usb_can_a::WaveshareUsbCanA::build_data_frame(
        usb_can::CanFrameType::Standard,
        0x7FFu32,
        &[0x01, 0x02, 0x03],
    )
    .unwrap();

    // Feed only part of the frame: nothing to parse yet
    client.io_mut().feed(&wire[..4]);
    assert!(client.try_read().is_none());

    // Feed the rest; now the frame completes
    client.io_mut().feed(&wire[4..]);
    let msg = client.poll_read().unwrap().expect("expected a message");
    assert_eq!(msg.id(), Id::Standard(StandardId::new(0x7FF).unwrap()));
    assert_eq!(msg.data(), &[0x01, 0x02, 0x03]);
}

#[test]
fn test_sync_skips_junk_bytes() {
    let mut client = CanUsbClient::new(MockIo::default(), wareshare_usb_can_a::WaveshareUsbCanA, config(), false).unwrap();

    let wire = wareshare_usb_can_a::WaveshareUsbCanA::build_data_frame(
        usb_can::CanFrameType::Standard,
        0x100u32,
        &[0x42],
    )
    .unwrap();
    let mut garbage = vec![0x00, 0xFF, 0x13];
    garbage.extend(&wire);
    client.io_mut().feed(&garbage);

    let msg = client.read().unwrap();
    assert_eq!(msg.data(), &[0x42]);
}

#[test]
fn test_async_client_roundtrip() {
    block_on(async {
        let mut client = AsyncCanUsbClient::new(MockIo::default(), wareshare_usb_can_a::WaveshareUsbCanA, config(), false).await.unwrap();
        assert_eq!(client.io().written, expected_settings());

        let wire = wareshare_usb_can_a::WaveshareUsbCanA::build_data_frame(
            usb_can::CanFrameType::Standard,
            0x321u32,
            &[0x55],
        )
        .unwrap();
        client.io_mut().feed(&wire);

        let msg = client.read().await.unwrap();
        assert_eq!(msg.id(), Id::Standard(StandardId::new(0x321).unwrap()));
        assert_eq!(msg.data(), &[0x55]);

        let frame = Frame::extended(ExtendedId::new(0x1ABCDE).unwrap(), &[1, 2, 3, 4]);
        client.write_frame(&frame).await.unwrap();
        assert!(client.io().written.len() > 20);
    });
}
