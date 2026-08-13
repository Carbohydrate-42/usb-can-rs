//! Protocol tests

use usb_can::protocol::wareshare_usb_can_a::WaveshareUsbCanA;
use usb_can::protocol::{ParsedFrame, Protocol};
use usb_can::{ExtendedId, Id, StandardId};

#[test]
fn test_build_settings_frame() {
    let frame = WaveshareUsbCanA::build_settings_frame(0x01, 0x00, false, 0x123, 0x7FF);

    assert_eq!(frame[0], 0xAA); // Start
    assert_eq!(frame[1], 0x55); // Command marker
    assert_eq!(frame[2], 0x12); // Command type
    assert_eq!(frame[3], 0x01); // Speed
    assert_eq!(frame[4], 0x01); // Frame type

    // Filter ID (5-8) - little endian: 0x123
    assert_eq!(frame[5], 0x23);
    assert_eq!(frame[6], 0x01);
    assert_eq!(frame[7], 0x00);
    assert_eq!(frame[8], 0x00);

    // Mask ID (9-12) - little endian: 0x7FF
    assert_eq!(frame[9], 0xFF);
    assert_eq!(frame[10], 0x07);
    assert_eq!(frame[11], 0x00);
    assert_eq!(frame[12], 0x00);
}

#[test]
fn test_build_data_frame_standard() {
    let data = WaveshareUsbCanA::build_data_frame(
        Id::Standard(StandardId::new(0x123).unwrap()),
        &[0x11, 0x22],
    )
    .unwrap();

    assert_eq!(data[0], 0xAA); // Start
    assert_eq!(data[1], 0xC2); // 0xC0 | DLC=2
    assert_eq!(data[2], 0x23); // ID LSB
    assert_eq!(data[3], 0x01); // ID MSB
    assert_eq!(data[4], 0x11); // Data
    assert_eq!(data[5], 0x22); // Data
    assert_eq!(data[6], 0x55); // Footer
}

#[test]
fn test_build_data_frame_extended() {
    let data = WaveshareUsbCanA::build_data_frame(
        Id::Extended(ExtendedId::new(0x12345).unwrap()),
        &[0xAA],
    )
    .unwrap();

    assert_eq!(data[0], 0xAA); // Start
    assert_eq!(data[1], 0xE1); // 0xC0 | 0x20 (extended) | DLC=1
    assert_eq!(data[4], 0xAA); // Data
}

#[test]
fn test_build_data_frame_too_long() {
    let result = WaveshareUsbCanA::build_data_frame(
        Id::Standard(StandardId::new(0x123).unwrap()),
        &[0u8; 9],
    );
    assert!(result.is_err());
}

#[test]
fn test_parse_single_frame() {
    // Standard frame: ID=0x123, DLC=2, Data=[0x11, 0x22]
    let buffer = [0xAA, 0xC2, 0x23, 0x01, 0x11, 0x22, 0x55];
    let mut output: Vec<ParsedFrame> = Vec::new();

    let consumed = WaveshareUsbCanA.parse_frames(&buffer, &mut output);

    assert_eq!(consumed, 7);
    assert_eq!(output.len(), 1);
    assert_eq!(output[0].id, 0x123);
    assert_eq!(output[0].data, vec![0x11, 0x22]);
    assert!(!output[0].is_extended);
}

#[test]
fn test_parse_multiple_frames() {
    // Two frames back-to-back
    let buffer = [
        0xAA, 0xC2, 0x23, 0x01, 0x11, 0x22, 0x55, // Frame 1: ID=0x123
        0xAA, 0xC1, 0x45, 0x06, 0xFF, 0x55,       // Frame 2: ID=0x645
    ];
    let mut output: Vec<ParsedFrame> = Vec::new();

    let consumed = WaveshareUsbCanA.parse_frames(&buffer, &mut output);

    assert_eq!(consumed, 13);
    assert_eq!(output.len(), 2);
    assert_eq!(output[0].id, 0x123);
    assert_eq!(output[1].id, 0x645);
}

#[test]
fn test_parse_extended_frame() {
    // Extended frame marker
    let buffer = [0xAA, 0xE2, 0x23, 0x01, 0x11, 0x22, 0x55];
    let mut output: Vec<ParsedFrame> = Vec::new();

    WaveshareUsbCanA.parse_frames(&buffer, &mut output);

    assert_eq!(output.len(), 1);
    assert!(output[0].is_extended);
}

#[test]
fn test_parse_incomplete_frame() {
    // Incomplete frame (missing footer)
    let buffer = [0xAA, 0xC2, 0x23, 0x01, 0x11, 0x22];
    let mut output: Vec<ParsedFrame> = Vec::new();

    let consumed = WaveshareUsbCanA.parse_frames(&buffer, &mut output);

    assert_eq!(consumed, 0); // Nothing consumed yet
    assert!(output.is_empty());
}

#[test]
fn test_parse_with_junk_bytes() {
    // Frame with junk bytes before it
    let buffer = [0x00, 0xFF, 0xAA, 0xC1, 0x23, 0x01, 0xAA, 0x55];
    let mut output: Vec<ParsedFrame> = Vec::new();

    let consumed = WaveshareUsbCanA.parse_frames(&buffer, &mut output);

    assert_eq!(consumed, 8); // All bytes consumed
    assert_eq!(output.len(), 1);
    assert_eq!(output[0].id, 0x123);
}

#[test]
fn test_parse_partial_consumption() {
    // One complete frame (DLC=2, 7 bytes), one incomplete
    let buffer = [
        0xAA, 0xC2, 0x23, 0x01, 0xAA, 0xBB, 0x55, // Complete frame: 2 data bytes
        0xAA, 0xC2, 0x45, 0x06,                   // Incomplete frame
    ];
    let mut output: Vec<ParsedFrame> = Vec::new();

    let consumed = WaveshareUsbCanA.parse_frames(&buffer, &mut output);

    assert_eq!(consumed, 7); // Only first frame consumed (7 bytes)
    assert_eq!(output.len(), 1);
}
