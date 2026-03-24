use super::*;

fn make_fragment(
    frame_id: u32,
    frag_index: u16,
    frag_total: u16,
    flags: u8,
    payload: Vec<u8>,
) -> VideoFragment {
    VideoFragment {
        frame_id,
        frag_index,
        frag_total,
        channel: Channel::Video,
        flags,
        payload,
    }
}

// ── Channel ───────────────────────────────────────────────────────────────

#[test]
fn test_channel_try_from_zero_is_video() {
    assert_eq!(Channel::try_from(0u8).unwrap(), Channel::Video);
}

#[test]
fn test_channel_try_from_unknown_returns_error() {
    let err = Channel::try_from(99u8).unwrap_err();
    assert!(matches!(err, TransportError::UnknownChannel(99)));
}

#[test]
fn test_channel_repr_is_zero() {
    assert_eq!(Channel::Video as u8, 0);
}

// ── VideoFragment::encode ─────────────────────────────────────────────────

#[test]
fn test_encode_produces_header_plus_payload() {
    let frag = make_fragment(1, 0, 1, 0, vec![0xAA, 0xBB]);
    let encoded = frag.encode();
    assert_eq!(encoded.len(), HEADER_LEN + 2);
}

#[test]
fn test_encode_frame_id_big_endian() {
    let frag = make_fragment(0x0102_0304, 0, 1, 0, vec![]);
    let encoded = frag.encode();
    assert_eq!(&encoded[0..4], &[0x01, 0x02, 0x03, 0x04]);
}

#[test]
fn test_encode_frag_index_big_endian() {
    let frag = make_fragment(0, 0x0506, 0x0607, 0, vec![]);
    let encoded = frag.encode();
    assert_eq!(&encoded[4..6], &[0x05, 0x06]);
}

#[test]
fn test_encode_frag_total_big_endian() {
    let frag = make_fragment(0, 0, 0x0102, 0, vec![]);
    let encoded = frag.encode();
    assert_eq!(&encoded[6..8], &[0x01, 0x02]);
}

#[test]
fn test_encode_channel_byte() {
    let frag = make_fragment(0, 0, 1, 0, vec![]);
    let encoded = frag.encode();
    assert_eq!(encoded[8], 0u8); // Channel::Video = 0
}

#[test]
fn test_encode_flags_byte() {
    let frag = make_fragment(0, 0, 1, FLAG_KEYFRAME, vec![]);
    let encoded = frag.encode();
    assert_eq!(encoded[9], FLAG_KEYFRAME);
}

#[test]
fn test_encode_reserved_bytes_are_zero() {
    let frag = make_fragment(0, 0, 1, 0, vec![]);
    let encoded = frag.encode();
    assert_eq!(&encoded[10..12], &[0x00, 0x00]);
}

#[test]
fn test_encode_payload_appended() {
    let payload = vec![1u8, 2, 3, 4];
    let frag = make_fragment(0, 0, 1, 0, payload.clone());
    let encoded = frag.encode();
    assert_eq!(&encoded[HEADER_LEN..], payload.as_slice());
}

#[test]
fn test_encode_empty_payload_produces_header_only() {
    let frag = make_fragment(0, 0, 1, 0, vec![]);
    let encoded = frag.encode();
    assert_eq!(encoded.len(), HEADER_LEN);
}

// ── VideoFragment::decode ─────────────────────────────────────────────────

#[test]
fn test_decode_roundtrip() {
    let frag = make_fragment(42, 1, 3, FLAG_KEYFRAME, vec![0xFF, 0x00]);
    let encoded = frag.encode();
    let decoded = VideoFragment::decode(&encoded).unwrap();
    assert_eq!(decoded, frag);
}

#[test]
fn test_decode_too_short_returns_error() {
    let buf = [0u8; HEADER_LEN - 1];
    let err = VideoFragment::decode(&buf).unwrap_err();
    assert!(matches!(err, TransportError::DatagramTooShort(11)));
}

#[test]
fn test_decode_empty_returns_error() {
    let err = VideoFragment::decode(&[]).unwrap_err();
    assert!(matches!(err, TransportError::DatagramTooShort(0)));
}

#[test]
fn test_decode_frag_total_zero_returns_error() {
    let mut buf = [0u8; HEADER_LEN];
    // frag_total is bytes 6..8, set to 0
    buf[6] = 0;
    buf[7] = 0;
    let err = VideoFragment::decode(&buf).unwrap_err();
    assert!(matches!(err, TransportError::InvalidFragTotal));
}

#[test]
fn test_decode_frag_index_out_of_range_returns_error() {
    let frag = make_fragment(0, 5, 3, 0, vec![]);
    let encoded = frag.encode();
    // Manually build with frag_index=5, frag_total=3 (invalid)
    let err = VideoFragment::decode(&encoded).unwrap_err();
    assert!(matches!(
        err,
        TransportError::FragIndexOutOfRange {
            frag_index: 5,
            frag_total: 3
        }
    ));
}

#[test]
fn test_decode_unknown_channel_returns_error() {
    let mut buf = [0u8; HEADER_LEN];
    buf[6] = 0; // frag_total high byte
    buf[7] = 1; // frag_total = 1
    buf[8] = 255; // unknown channel
    let err = VideoFragment::decode(&buf).unwrap_err();
    assert!(matches!(err, TransportError::UnknownChannel(255)));
}

#[test]
fn test_decode_exact_header_no_payload() {
    let frag = make_fragment(99, 0, 1, 0, vec![]);
    let encoded = frag.encode();
    let decoded = VideoFragment::decode(&encoded).unwrap();
    assert!(decoded.payload.is_empty());
}

#[test]
fn test_decode_reserved_bytes_ignored() {
    let frag = make_fragment(1, 0, 1, 0, vec![0xAB]);
    let mut encoded = frag.encode().to_vec();
    // Corrupt reserved bytes — should still decode fine
    encoded[10] = 0xDE;
    encoded[11] = 0xAD;
    let decoded = VideoFragment::decode(&encoded).unwrap();
    assert_eq!(decoded.payload, vec![0xAB]);
}

// ── VideoFragment::is_keyframe ─────────────────────────────────────────────

#[test]
fn test_is_keyframe_true_when_flag_set() {
    let frag = make_fragment(0, 0, 1, FLAG_KEYFRAME, vec![]);
    assert!(frag.is_keyframe());
}

#[test]
fn test_is_keyframe_false_when_flag_not_set() {
    let frag = make_fragment(0, 0, 1, 0, vec![]);
    assert!(!frag.is_keyframe());
}

#[test]
fn test_is_keyframe_only_checks_bit_zero() {
    // bit 1 set, bit 0 clear → not a keyframe
    let frag = make_fragment(0, 0, 1, 0b0000_0010, vec![]);
    assert!(!frag.is_keyframe());
}

// ── Constants ─────────────────────────────────────────────────────────────

#[test]
fn test_header_len_is_twelve() {
    assert_eq!(HEADER_LEN, 12);
}

#[test]
fn test_max_fragment_payload_is_correct() {
    assert_eq!(MAX_FRAGMENT_PAYLOAD, 1200 - 12);
}

#[test]
fn test_flag_keyframe_is_bit_zero() {
    assert_eq!(FLAG_KEYFRAME, 1);
}

// ── TransportError display ─────────────────────────────────────────────────

#[test]
fn test_transport_error_datagram_too_short_display() {
    let e = TransportError::DatagramTooShort(5);
    assert!(e.to_string().contains('5'));
}

#[test]
fn test_transport_error_invalid_frag_total_display() {
    let e = TransportError::InvalidFragTotal;
    assert!(e.to_string().contains("frag_total"));
}

#[test]
fn test_transport_error_frag_index_out_of_range_display() {
    let e = TransportError::FragIndexOutOfRange {
        frag_index: 3,
        frag_total: 2,
    };
    let s = e.to_string();
    assert!(s.contains('3') && s.contains('2'));
}

#[test]
fn test_transport_error_tls_error_display() {
    let e = TransportError::TlsError("bad cert".to_string());
    assert!(e.to_string().contains("bad cert"));
}

#[test]
fn test_transport_error_storage_error_display() {
    let e = TransportError::StorageError("disk full".to_string());
    assert!(e.to_string().contains("disk full"));
}

#[test]
fn test_transport_error_endpoint_closed_display() {
    let e = TransportError::EndpointClosed;
    assert_eq!(e.to_string(), "endpoint closed");
}

#[test]
fn test_transport_error_stream_write_display() {
    let e = TransportError::StreamWrite("broken pipe".to_string());
    assert_eq!(e.to_string(), "stream write error: broken pipe");
}

#[test]
fn test_transport_error_stream_read_display() {
    let e = TransportError::StreamRead("reset".to_string());
    assert_eq!(e.to_string(), "stream read error: reset");
}

#[test]
fn test_transport_error_message_too_large_display() {
    let e = TransportError::MessageTooLarge(100_000);
    assert!(e.to_string().contains("100000"));
}

#[test]
fn test_transport_error_message_parse_display() {
    let e = TransportError::MessageParse("invalid json".to_string());
    assert!(e.to_string().contains("invalid json"));
}
