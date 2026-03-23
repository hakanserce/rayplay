use super::*;
use crate::packet::EncodedPacket;

// ── annex_b_to_length_prefixed ─────────────────────────────────────────

#[test]
fn test_annex_b_to_length_prefixed_empty_input_returns_empty() {
    assert!(annex_b_to_length_prefixed(&[]).is_empty());
}

#[test]
fn test_annex_b_to_length_prefixed_4byte_start_code_replaced_with_length() {
    let input = [0x00u8, 0x00, 0x00, 0x01, 0x26, 0x01];
    let out = annex_b_to_length_prefixed(&input);
    assert_eq!(&out[..4], &[0, 0, 0, 2]);
    assert_eq!(&out[4..], &[0x26, 0x01]);
}

#[test]
fn test_annex_b_to_length_prefixed_3byte_start_code_replaced_with_length() {
    let input = [0x00u8, 0x00, 0x01, 0x26, 0x01];
    let out = annex_b_to_length_prefixed(&input);
    assert_eq!(&out[..4], &[0, 0, 0, 2]);
    assert_eq!(&out[4..], &[0x26, 0x01]);
}

#[test]
fn test_annex_b_to_length_prefixed_two_nal_units() {
    let input = [
        0x00u8, 0x00, 0x00, 0x01, 0xAA, // NAL 1 (1 byte)
        0x00, 0x00, 0x00, 0x01, 0xBB, 0xCC, // NAL 2 (2 bytes)
    ];
    let out = annex_b_to_length_prefixed(&input);
    assert_eq!(&out[..4], &[0, 0, 0, 1]);
    assert_eq!(out[4], 0xAA);
    assert_eq!(&out[5..9], &[0, 0, 0, 2]);
    assert_eq!(&out[9..], &[0xBB, 0xCC]);
}

#[test]
fn test_annex_b_to_length_prefixed_data_without_start_codes_returns_empty() {
    // No start codes → split_nal_units returns [] → no output
    let input = [0xAAu8, 0xBB, 0xCC];
    assert!(annex_b_to_length_prefixed(&input).is_empty());
}

#[test]
fn test_annex_b_to_length_prefixed_trailing_start_code_with_no_nal_bytes_returns_empty() {
    // Start code at end with no bytes following — produces no NAL unit.
    let input = [0x00u8, 0x00, 0x00, 0x01];
    assert!(annex_b_to_length_prefixed(&input).is_empty());
}

// ── split_nal_units ────────────────────────────────────────────────────

#[test]
fn test_split_nal_units_empty_returns_empty() {
    assert!(split_nal_units(&[]).is_empty());
}

#[test]
fn test_split_nal_units_single_4byte_start_code() {
    let input = [0x00u8, 0x00, 0x00, 0x01, 0x40, 0x01];
    let nals = split_nal_units(&input);
    assert_eq!(nals.len(), 1);
    assert_eq!(nals[0], &[0x40u8, 0x01]);
}

#[test]
fn test_split_nal_units_single_3byte_start_code() {
    let input = [0x00u8, 0x00, 0x01, 0x40, 0x01];
    let nals = split_nal_units(&input);
    assert_eq!(nals.len(), 1);
    assert_eq!(nals[0], &[0x40u8, 0x01]);
}

#[test]
fn test_split_nal_units_two_nals() {
    let input = [
        0x00u8, 0x00, 0x00, 0x01, 0x40, // VPS
        0x00, 0x00, 0x00, 0x01, 0x42, // SPS
    ];
    let nals = split_nal_units(&input);
    assert_eq!(nals.len(), 2);
    assert_eq!(nals[0], &[0x40u8]);
    assert_eq!(nals[1], &[0x42u8]);
}

#[test]
fn test_split_nal_units_two_3byte_start_codes() {
    // Both NAL units use 3-byte start codes; the second start code must
    // push the first NAL (line 263 in split_nal_units).
    let input = [
        0x00u8, 0x00, 0x01, 0x40, // first NAL: 3-byte start + payload
        0x00, 0x00, 0x01, 0x42, // second NAL: 3-byte start + payload
    ];
    let nals = split_nal_units(&input);
    assert_eq!(nals.len(), 2);
    assert_eq!(nals[0], &[0x40u8]);
    assert_eq!(nals[1], &[0x42u8]);
}

#[test]
fn test_split_nal_units_no_trailing_start_code() {
    let input = [0x00u8, 0x00, 0x00, 0x01, 0x44, 0x01, 0x02];
    let nals = split_nal_units(&input);
    assert_eq!(nals.len(), 1);
    assert_eq!(nals[0], &[0x44u8, 0x01, 0x02]);
}

#[test]
fn test_split_nal_units_no_start_codes_returns_empty() {
    assert!(split_nal_units(&[0xAAu8, 0xBB]).is_empty());
}

// ── is_hevc_parameter_set ──────────────────────────────────────────────

#[test]
fn test_is_hevc_parameter_set_vps_type_32() {
    assert!(is_hevc_parameter_set(&[0x40, 0x01])); // (32 << 1) = 0x40
}

#[test]
fn test_is_hevc_parameter_set_sps_type_33() {
    assert!(is_hevc_parameter_set(&[0x42, 0x01])); // (33 << 1) = 0x42
}

#[test]
fn test_is_hevc_parameter_set_pps_type_34() {
    assert!(is_hevc_parameter_set(&[0x44, 0x01])); // (34 << 1) = 0x44
}

#[test]
fn test_is_hevc_parameter_set_idr_not_param_set() {
    assert!(!is_hevc_parameter_set(&[0x26, 0x01])); // NAL type 19 = IDR
}

#[test]
fn test_is_hevc_parameter_set_empty_returns_false() {
    assert!(!is_hevc_parameter_set(&[]));
}

// ── is_h264_parameter_set ──────────────────────────────────────────────

#[test]
fn test_is_h264_parameter_set_sps_type_7() {
    assert!(is_h264_parameter_set(&[0x67, 0x01])); // NAL type 7 = SPS
}

#[test]
fn test_is_h264_parameter_set_pps_type_8() {
    assert!(is_h264_parameter_set(&[0x68, 0x01])); // NAL type 8 = PPS
}

#[test]
fn test_is_h264_parameter_set_idr_not_param_set() {
    assert!(!is_h264_parameter_set(&[0x65, 0x01])); // NAL type 5 = IDR
}

#[test]
fn test_is_h264_parameter_set_empty_returns_false() {
    assert!(!is_h264_parameter_set(&[]));
}

// ── VtDecoder lifecycle ────────────────────────────────────────────────

#[test]
fn test_vt_decoder_new_hevc_returns_ok() {
    let dec = VtDecoder::new(Codec::Hevc).unwrap();
    assert_eq!(dec.codec(), Codec::Hevc);
}

#[test]
fn test_vt_decoder_new_h264_returns_ok() {
    let dec = VtDecoder::new(Codec::H264).unwrap();
    assert_eq!(dec.codec(), Codec::H264);
}

#[test]
fn test_vt_decoder_is_session_ready_false_after_new() {
    let dec = VtDecoder::new(Codec::Hevc).unwrap();
    assert!(!dec.is_session_ready());
}

#[test]
fn test_vt_decoder_decode_non_keyframe_without_session_returns_error() {
    let mut dec = VtDecoder::new(Codec::Hevc).unwrap();
    let packet = EncodedPacket::new(vec![0u8; 64], false, 0, 16_667);
    let err = dec.decode(&packet).unwrap_err();
    assert!(matches!(err, VideoError::DecodingFailed { .. }));
    assert!(err.to_string().contains("keyframe"));
}

#[test]
fn test_vt_decoder_decode_hevc_keyframe_no_param_sets_returns_corrupt_packet() {
    let mut dec = VtDecoder::new(Codec::Hevc).unwrap();
    // IDR slice only — no VPS/SPS/PPS
    let idr = vec![0x00u8, 0x00, 0x00, 0x01, 0x26, 0x01, 0x00];
    let packet = EncodedPacket::new(idr, true, 0, 16_667);
    let err = dec.decode(&packet).unwrap_err();
    assert!(matches!(err, VideoError::CorruptPacket { .. }));
    assert!(err.to_string().contains("HEVC parameter sets"));
}

#[test]
fn test_vt_decoder_decode_h264_keyframe_no_param_sets_returns_corrupt_packet() {
    let mut dec = VtDecoder::new(Codec::H264).unwrap();
    // IDR slice only — no SPS/PPS
    let idr = vec![0x00u8, 0x00, 0x00, 0x01, 0x65, 0x01, 0x00];
    let packet = EncodedPacket::new(idr, true, 0, 16_667);
    let err = dec.decode(&packet).unwrap_err();
    assert!(matches!(err, VideoError::CorruptPacket { .. }));
    assert!(err.to_string().contains("H.264 parameter sets"));
}

#[test]
fn test_vt_decoder_decode_hevc_keyframe_with_param_sets_returns_decoding_failed() {
    let mut dec = VtDecoder::new(Codec::Hevc).unwrap();
    // Fake VPS + SPS + PPS NALs — real VT will reject invalid SPS data.
    let packet_data = vec![
        0x00u8, 0x00, 0x00, 0x01, 0x40, 0x01, // fake VPS
        0x00, 0x00, 0x00, 0x01, 0x42, 0x01, // fake SPS
        0x00, 0x00, 0x00, 0x01, 0x44, 0x01, // fake PPS
    ];
    let packet = EncodedPacket::new(packet_data, true, 0, 16_667);
    let err = dec.decode(&packet).unwrap_err();
    // Either CMVideoFormatDescriptionCreate fails (hw build) or
    // we hit the non-hw fallback error (non-hw build).
    assert!(matches!(
        err,
        VideoError::DecodingFailed { .. } | VideoError::CorruptPacket { .. }
    ));
}

#[test]
fn test_vt_decoder_decode_h264_keyframe_with_param_sets_returns_decoding_failed() {
    let mut dec = VtDecoder::new(Codec::H264).unwrap();
    // Fake SPS + PPS NALs — real VT will reject invalid SPS data.
    let packet_data = vec![
        0x00u8, 0x00, 0x00, 0x01, 0x67, 0x01, // fake SPS
        0x00, 0x00, 0x00, 0x01, 0x68, 0x01, // fake PPS
    ];
    let packet = EncodedPacket::new(packet_data, true, 0, 16_667);
    let err = dec.decode(&packet).unwrap_err();
    // Either CMVideoFormatDescriptionCreate fails (hw build) or
    // we hit the non-hw fallback error (non-hw build).
    assert!(matches!(
        err,
        VideoError::DecodingFailed { .. } | VideoError::CorruptPacket { .. }
    ));
}

#[test]
fn test_vt_decoder_flush_returns_empty() {
    let mut dec = VtDecoder::new(Codec::Hevc).unwrap();
    assert!(dec.flush().unwrap().is_empty());
}

#[test]
fn test_vt_decoder_codec_is_hevc() {
    let dec = VtDecoder::new(Codec::Hevc).unwrap();
    assert_eq!(dec.codec(), Codec::Hevc);
}

#[test]
fn test_vt_decoder_codec_is_h264() {
    let dec = VtDecoder::new(Codec::H264).unwrap();
    assert_eq!(dec.codec(), Codec::H264);
}

#[test]
fn test_vt_decoder_drop_without_session_does_not_panic() {
    let dec = VtDecoder::new(Codec::Hevc).unwrap();
    drop(dec);
}
