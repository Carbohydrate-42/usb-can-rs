//! Tests for the zencan frontend message conversions.

use usb_can::frontends::zencan::{message_from_zencan, message_to_zencan};
use usb_can::{ExtendedId, Id, StandardId};
use zencan_common::{CanId, CanMessage as ZenCanMessage};

#[test]
fn test_standard_message_roundtrip() {
    let zen = ZenCanMessage::new(CanId::Std(0x123), &[0x11, 0x22, 0x33]);
    let ours = message_from_zencan(&zen).unwrap();

    assert_eq!(ours.id(), Id::Standard(StandardId::new(0x123).unwrap()));
    assert_eq!(ours.data(), &[0x11, 0x22, 0x33]);
    assert!(!ours.is_rtr());

    let back = message_to_zencan(&ours);
    assert_eq!(back.id, CanId::Std(0x123));
    assert_eq!(back.data(), &[0x11, 0x22, 0x33]);
    assert!(!back.rtr);
}

#[test]
fn test_extended_message_roundtrip() {
    let zen = ZenCanMessage::new(CanId::Extended(0x1ABCDE), &[0xAA]);
    let ours = message_from_zencan(&zen).unwrap();

    assert_eq!(ours.id(), Id::Extended(ExtendedId::new(0x1ABCDE).unwrap()));
    assert_eq!(ours.data(), &[0xAA]);

    let back = message_to_zencan(&ours);
    assert_eq!(back.id, CanId::Extended(0x1ABCDE));
    assert_eq!(back.data(), &[0xAA]);
}

#[test]
fn test_rtr_message_roundtrip() {
    let zen = ZenCanMessage::new_rtr(CanId::Std(0x100));
    let ours = message_from_zencan(&zen).unwrap();

    assert!(ours.is_rtr());
    assert_eq!(ours.data(), &[]);

    let back = message_to_zencan(&ours);
    assert!(back.rtr);
    assert_eq!(back.id, CanId::Std(0x100));
}

#[test]
fn test_invalid_standard_id_rejected() {
    // 0x800 does not fit in 11 bits
    let zen = ZenCanMessage::new(CanId::Std(0x800), &[]);
    assert!(message_from_zencan(&zen).is_none());
}

#[test]
fn test_empty_data_roundtrip() {
    let zen = ZenCanMessage::new(CanId::Std(0x1), &[]);
    let ours = message_from_zencan(&zen).unwrap();
    assert_eq!(ours.dlc(), 0);

    let back = message_to_zencan(&ours);
    assert_eq!(back.data(), &[]);
}
