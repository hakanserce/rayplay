use super::*;
use crate::packet::EncodedPacket;

// ── Test helper ───────────────────────────────────────────────────────

/// Converts Annex B to length-prefixed format (all NAL units, no filtering).
/// Used only to test the `split_nal_units` + length-prefix logic in isolation.
#[allow(clippy::cast_possible_truncation)]
fn annex_b_to_length_prefixed(data: &[u8]) -> Vec<u8> {
    let nals = split_nal_units(data);
    let mut out = Vec::with_capacity(data.len());
    for nal in nals {
        if !nal.is_empty() {
            let len = nal.len() as u32;
            out.extend_from_slice(&len.to_be_bytes());
            out.extend_from_slice(nal);
        }
    }
    out
}

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

// ── annex_b_to_vcl_length_prefixed ─────────────────────────────────────

#[test]
fn test_annex_b_to_vcl_length_prefixed_empty_input_returns_empty() {
    assert!(annex_b_to_vcl_length_prefixed(&[], Codec::Hevc).is_empty());
    assert!(annex_b_to_vcl_length_prefixed(&[], Codec::H264).is_empty());
}

#[test]
fn test_annex_b_to_vcl_length_prefixed_hevc_keyframe_keeps_only_idr() {
    // Pattern: [AUD][VPS][SPS][PPS][SEI][IDR] - keeps only IDR
    let input = [
        0x00u8, 0x00, 0x00, 0x01, 0x46, 0x01, // AUD (type 35)
        0x00, 0x00, 0x00, 0x01, 0x40, 0x01, // VPS (type 32)
        0x00, 0x00, 0x00, 0x01, 0x42, 0x01, // SPS (type 33)
        0x00, 0x00, 0x00, 0x01, 0x44, 0x01, // PPS (type 34)
        0x00, 0x00, 0x00, 0x01, 0x4E, 0x01, // SEI (type 39)
        0x00, 0x00, 0x00, 0x01, 0x26, 0x01, // IDR (type 19)
    ];
    let out = annex_b_to_vcl_length_prefixed(&input, Codec::Hevc);
    // Should contain only the IDR NAL unit
    assert_eq!(out.len(), 6); // 4-byte length + 2-byte IDR payload
    assert_eq!(&out[..4], &[0, 0, 0, 2]); // length prefix
    assert_eq!(&out[4..], &[0x26, 0x01]); // IDR payload
}

#[test]
fn test_annex_b_to_vcl_length_prefixed_hevc_p_frame_keeps_only_trail_r() {
    // Pattern: [AUD][SEI][TRAIL_R] - keeps only TRAIL_R
    let input = [
        0x00u8, 0x00, 0x00, 0x01, 0x46, 0x01, // AUD (type 35)
        0x00, 0x00, 0x00, 0x01, 0x4E, 0x01, // SEI (type 39)
        0x00, 0x00, 0x00, 0x01, 0x02, 0x01, // TRAIL_R (type 1)
    ];
    let out = annex_b_to_vcl_length_prefixed(&input, Codec::Hevc);
    // Should contain only the TRAIL_R NAL unit
    assert_eq!(out.len(), 6); // 4-byte length + 2-byte TRAIL_R payload
    assert_eq!(&out[..4], &[0, 0, 0, 2]); // length prefix
    assert_eq!(&out[4..], &[0x02, 0x01]); // TRAIL_R payload
}

#[test]
fn test_annex_b_to_vcl_length_prefixed_hevc_all_non_vcl_returns_empty() {
    let input = [
        0x00u8, 0x00, 0x00, 0x01, 0x40, 0x01, // VPS (type 32)
        0x00, 0x00, 0x00, 0x01, 0x46, 0x01, // AUD (type 35)
        0x00, 0x00, 0x00, 0x01, 0x4E, 0x01, // SEI (type 39)
    ];
    assert!(annex_b_to_vcl_length_prefixed(&input, Codec::Hevc).is_empty());
}

#[test]
fn test_annex_b_to_vcl_length_prefixed_h264_keyframe_keeps_only_idr() {
    // Pattern: [AUD][SPS][PPS][IDR] - keeps only IDR
    let input = [
        0x00u8, 0x00, 0x00, 0x01, 0x69, 0x01, // AUD (type 9)
        0x00, 0x00, 0x00, 0x01, 0x67, 0x01, // SPS (type 7)
        0x00, 0x00, 0x00, 0x01, 0x68, 0x01, // PPS (type 8)
        0x00, 0x00, 0x00, 0x01, 0x65, 0x01, // IDR (type 5)
    ];
    let out = annex_b_to_vcl_length_prefixed(&input, Codec::H264);
    // Should contain only the IDR NAL unit
    assert_eq!(out.len(), 6); // 4-byte length + 2-byte IDR payload
    assert_eq!(&out[..4], &[0, 0, 0, 2]); // length prefix
    assert_eq!(&out[4..], &[0x65, 0x01]); // IDR payload
}

#[test]
fn test_annex_b_to_vcl_length_prefixed_h264_p_frame_keeps_only_slice() {
    // Pattern: [AUD][slice] - keeps only slice
    let input = [
        0x00u8, 0x00, 0x00, 0x01, 0x69, 0x01, // AUD (type 9)
        0x00, 0x00, 0x00, 0x01, 0x61, 0x01, // slice (type 1)
    ];
    let out = annex_b_to_vcl_length_prefixed(&input, Codec::H264);
    // Should contain only the slice NAL unit
    assert_eq!(out.len(), 6); // 4-byte length + 2-byte slice payload
    assert_eq!(&out[..4], &[0, 0, 0, 2]); // length prefix
    assert_eq!(&out[4..], &[0x61, 0x01]); // slice payload
}

#[test]
fn test_annex_b_to_vcl_length_prefixed_h264_all_non_vcl_returns_empty() {
    let input = [
        0x00u8, 0x00, 0x00, 0x01, 0x67, 0x01, // SPS (type 7)
        0x00, 0x00, 0x00, 0x01, 0x69, 0x01, // AUD (type 9)
        0x00, 0x00, 0x00, 0x01, 0x60, 0x01, // undefined type 0
    ];
    assert!(annex_b_to_vcl_length_prefixed(&input, Codec::H264).is_empty());
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

// ── is_hevc_vcl_nal ────────────────────────────────────────────────────

#[test]
fn test_is_hevc_vcl_nal_idr_type_19() {
    assert!(is_hevc_vcl_nal(&[0x26, 0x01])); // type 19: (19 << 1) = 0x26
}

#[test]
fn test_is_hevc_vcl_nal_trail_r_type_1() {
    assert!(is_hevc_vcl_nal(&[0x02, 0x01])); // type 1: (1 << 1) = 0x02
}

#[test]
fn test_is_hevc_vcl_nal_boundary_type_31() {
    assert!(is_hevc_vcl_nal(&[0x3E, 0x01])); // type 31: (31 << 1) = 0x3E
}

#[test]
fn test_is_hevc_vcl_nal_type_0() {
    assert!(is_hevc_vcl_nal(&[0x00, 0x01])); // type 0: (0 << 1) = 0x00
}

#[test]
fn test_is_hevc_vcl_nal_vps_type_32_not_vcl() {
    assert!(!is_hevc_vcl_nal(&[0x40, 0x01])); // type 32: (32 << 1) = 0x40
}

#[test]
fn test_is_hevc_vcl_nal_aud_type_35_not_vcl() {
    assert!(!is_hevc_vcl_nal(&[0x46, 0x01])); // type 35: (35 << 1) = 0x46
}

#[test]
fn test_is_hevc_vcl_nal_sei_type_39_not_vcl() {
    assert!(!is_hevc_vcl_nal(&[0x4E, 0x01])); // type 39: (39 << 1) = 0x4E
}

#[test]
fn test_is_hevc_vcl_nal_empty_returns_false() {
    assert!(!is_hevc_vcl_nal(&[]));
}

// ── is_h264_vcl_nal ────────────────────────────────────────────────────

#[test]
fn test_is_h264_vcl_nal_idr_type_5() {
    assert!(is_h264_vcl_nal(&[0x65, 0x01])); // type 5 = IDR
}

#[test]
fn test_is_h264_vcl_nal_slice_type_1() {
    assert!(is_h264_vcl_nal(&[0x61, 0x01])); // type 1 = slice
}

#[test]
fn test_is_h264_vcl_nal_slice_type_2() {
    assert!(is_h264_vcl_nal(&[0x62, 0x01])); // type 2 = slice
}

#[test]
fn test_is_h264_vcl_nal_slice_type_3() {
    assert!(is_h264_vcl_nal(&[0x63, 0x01])); // type 3 = slice
}

#[test]
fn test_is_h264_vcl_nal_slice_type_4() {
    assert!(is_h264_vcl_nal(&[0x64, 0x01])); // type 4 = slice
}

#[test]
fn test_is_h264_vcl_nal_sps_type_7_not_vcl() {
    assert!(!is_h264_vcl_nal(&[0x67, 0x01])); // type 7 = SPS
}

#[test]
fn test_is_h264_vcl_nal_aud_type_9_not_vcl() {
    assert!(!is_h264_vcl_nal(&[0x69, 0x01])); // type 9 = AUD
}

#[test]
fn test_is_h264_vcl_nal_type_0_not_vcl() {
    assert!(!is_h264_vcl_nal(&[0x60, 0x01])); // type 0 = undefined
}

#[test]
fn test_is_h264_vcl_nal_empty_returns_false() {
    assert!(!is_h264_vcl_nal(&[]));
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

// ── Hardware-specific tests (require hw-codec-tests feature) ───────────

#[cfg(feature = "hw-codec-tests")]
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
    // CMVideoFormatDescriptionCreate will fail with invalid SPS data.
    assert!(matches!(err, VideoError::DecodingFailed { .. }));
}

#[cfg(feature = "hw-codec-tests")]
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
    // CMVideoFormatDescriptionCreate will fail with invalid SPS data.
    assert!(matches!(err, VideoError::DecodingFailed { .. }));
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
