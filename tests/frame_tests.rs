//! Frame builder tests

use usb_can_a::{CanId, Frame};

#[test]
fn test_frame_standard() {
    let frame = Frame::standard(CanId::Std(0x123), &[0x11, 0x22]);
    assert_eq!(frame.id(), CanId::Std(0x123));
    assert_eq!(frame.data(), &[0x11, 0x22]);
    assert!(!frame.is_extended());
}

#[test]
fn test_frame_extended() {
    let frame = Frame::extended(CanId::Extended(0x12345), &[0x33, 0x44]);
    assert_eq!(frame.id(), CanId::Extended(0x12345));
    assert_eq!(frame.data(), &[0x33, 0x44]);
    assert!(frame.is_extended());
}

#[test]
fn test_frame_with_id_standard() {
    let frame = Frame::with_id(0x123, &[0x11, 0x22]);
    assert_eq!(frame.id(), CanId::Std(0x123));
    assert!(!frame.is_extended());
}

#[test]
fn test_frame_with_id_extended() {
    let frame = Frame::with_id(0x12345, &[0x33, 0x44]);
    assert_eq!(frame.id(), CanId::Extended(0x12345));
    assert!(frame.is_extended());
}

#[test]
fn test_frame_from_hex() {
    let frame = Frame::from_hex("123", "DEADBEEF").unwrap();
    assert_eq!(frame.id(), CanId::Std(0x123));
    assert_eq!(frame.data(), &[0xDE, 0xAD, 0xBE, 0xEF]);
}

#[test]
fn test_frame_from_hex_extended() {
    let frame = Frame::from_hex("12345", "AABBCCDD").unwrap();
    assert_eq!(frame.id(), CanId::Extended(0x12345));
    assert_eq!(frame.data(), &[0xAA, 0xBB, 0xCC, 0xDD]);
}

#[test]
fn test_frame_from_hex_with_spaces() {
    let frame = Frame::from_hex("7FF", "DE AD BE EF").unwrap();
    assert_eq!(frame.id(), CanId::Std(0x7FF));
    assert_eq!(frame.data(), &[0xDE, 0xAD, 0xBE, 0xEF]);
}

#[test]
fn test_frame_from_hex_invalid() {
    // Odd length data
    assert!(Frame::from_hex("123", "ABC").is_none());
    // Invalid hex
    assert!(Frame::from_hex("XYZ", "1234").is_none());
}

#[test]
fn test_frame_rtr() {
    let frame = Frame::rtr(CanId::Std(0x100));
    assert!(frame.is_rtr());
    assert_eq!(frame.dlc(), 0);
}

#[test]
fn test_frame_from_message() {
    use usb_can_a::CanMessage;
    
    let msg = CanMessage::new(CanId::Std(0x200), &[0x01, 0x02, 0x03]);
    let frame = Frame::from_message(msg.clone());
    
    assert_eq!(frame.id(), msg.id);
    assert_eq!(frame.data(), msg.data());
}

#[test]
fn test_frame_conversions() {
    use usb_can_a::CanMessage;
    
    let frame = Frame::standard(CanId::Std(0x300), &[0xAA, 0xBB]);
    let msg: CanMessage = frame.into();
    
    assert_eq!(msg.id, CanId::Std(0x300));
    assert_eq!(msg.data(), &[0xAA, 0xBB]);
}
