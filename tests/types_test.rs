//! Type tests

use usb_can::protocol::waveshare_usb_can_a::{CanMode, CanSpeed};

#[test]
fn test_can_speed_try_from() {
    assert_eq!(CanSpeed::try_from(1000000u32).unwrap(), CanSpeed::Bps1000000);
    assert_eq!(CanSpeed::try_from(500000u32).unwrap(), CanSpeed::Bps500000);
    assert_eq!(CanSpeed::try_from(250000u32).unwrap(), CanSpeed::Bps250000);
    assert_eq!(CanSpeed::try_from(125000u32).unwrap(), CanSpeed::Bps125000);
}

#[test]
fn test_can_speed_try_from_invalid() {
    assert!(CanSpeed::try_from(123456u32).is_err());
    assert!(CanSpeed::try_from(0u32).is_err());
}

#[test]
fn test_can_speed_as_bps() {
    assert_eq!(CanSpeed::Bps1000000.as_bps(), 1000000);
    assert_eq!(CanSpeed::Bps500000.as_bps(), 500000);
    assert_eq!(CanSpeed::Bps250000.as_bps(), 250000);
}

#[test]
fn test_can_mode_values() {
    assert_eq!(CanMode::Normal as u8, 0x00);
    assert_eq!(CanMode::Loopback as u8, 0x01);
    assert_eq!(CanMode::Silent as u8, 0x02);
    assert_eq!(CanMode::LoopbackSilent as u8, 0x03);
}
