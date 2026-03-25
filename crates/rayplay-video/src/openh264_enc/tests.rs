use super::*;
use crate::encoder::{Bitrate, Codec};
use crate::frame::RawFrame;
use openh264::formats::YUVSource;

fn make_config(width: u32, height: u32, fps: u32) -> EncoderConfig {
    EncoderConfig::with_codec(width, height, fps, Codec::H264)
}

#[test]
fn test_openh264_encoder_new_creates_session() {
    let enc = OpenH264Encoder::new(make_config(64, 64, 30));
    assert!(enc.is_ok());
}

#[test]
fn test_openh264_encoder_rejects_hevc() {
    let config = EncoderConfig::new(64, 64, 30); // defaults to HEVC
    let err = OpenH264Encoder::new(config).unwrap_err();
    assert!(matches!(err, VideoError::UnsupportedCodec { .. }));
}

#[test]
fn test_openh264_encoder_rejects_gpu_texture() {
    let mut enc = OpenH264Encoder::new(make_config(64, 64, 30)).unwrap();
    let input = EncoderInput::GpuTexture {
        handle: crate::encoder::GpuTextureHandle(std::ptr::null_mut()),
        width: 64,
        height: 64,
        timestamp_us: 0,
    };
    let err = enc.encode(input).unwrap_err();
    assert!(matches!(err, VideoError::EncodingFailed { .. }));
}

#[test]
fn test_openh264_encoder_rejects_wrong_dimensions() {
    let mut enc = OpenH264Encoder::new(make_config(64, 64, 30)).unwrap();
    let frame = RawFrame::new(vec![0u8; 128 * 128 * 4], 128, 128, 128 * 4, 0);
    let err = enc.encode(EncoderInput::Cpu(&frame)).unwrap_err();
    assert!(matches!(err, VideoError::InvalidDimensions { .. }));
}

#[test]
fn test_openh264_encoder_encodes_small_frame() {
    let mut enc = OpenH264Encoder::new(make_config(64, 64, 30)).unwrap();
    let frame = RawFrame::new(vec![128u8; 64 * 64 * 4], 64, 64, 64 * 4, 1000);
    let result = enc.encode(EncoderInput::Cpu(&frame)).unwrap();
    assert!(result.is_some());
    let packet = result.unwrap();
    assert!(!packet.data.is_empty());
    assert_eq!(packet.timestamp_us, 1000);
}

#[test]
fn test_openh264_encoder_flush_returns_empty() {
    let mut enc = OpenH264Encoder::new(make_config(64, 64, 30)).unwrap();
    assert!(enc.flush().unwrap().is_empty());
}

#[test]
fn test_openh264_encoder_config_accessor() {
    let config = make_config(320, 240, 60).with_bitrate(Bitrate::Mbps(5));
    let enc = OpenH264Encoder::new(config).unwrap();
    assert_eq!(enc.config().width, 320);
    assert_eq!(enc.config().height, 240);
    assert_eq!(enc.config().fps, 60);
    assert_eq!(enc.config().codec, Codec::H264);
    assert_eq!(enc.config().bitrate, Bitrate::Mbps(5));
}

#[test]
fn test_openh264_encoder_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<OpenH264Encoder>();
}

#[test]
fn test_bgra_to_yuv_white_pixel() {
    // Pure white BGRA pixel → Y should be close to 255 (full-range BT.601)
    let data = vec![
        255u8, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ];
    let yuv = bgra_to_yuv(&data, 2, 2, 2 * 4);
    let y_data = yuv.y();
    assert!(
        y_data[0] > 200,
        "Y value {} too low for white pixel",
        y_data[0]
    );
}

#[test]
fn test_bgra_to_yuv_dimensions() {
    let w = 4_u32;
    let h = 4_u32;
    let data = vec![128u8; (w * h * 4) as usize];
    let yuv = bgra_to_yuv(&data, w, h, w * 4);
    let (dw, dh) = yuv.dimensions();
    assert_eq!(dw, w as usize);
    assert_eq!(dh, h as usize);
}

#[test]
fn test_bitstream_to_vec_from_encode() {
    let mut enc = OpenH264Encoder::new(make_config(64, 64, 30)).unwrap();
    let frame = RawFrame::new(vec![100u8; 64 * 64 * 4], 64, 64, 64 * 4, 0);
    let yuv = bgra_to_yuv(&frame.data, 64, 64, 64 * 4);
    let bs = enc.encoder.encode(&yuv).unwrap();
    let data = bs.to_vec();
    assert!(!data.is_empty());
}

#[test]
fn test_openh264_encoder_debug_impl() {
    let enc = OpenH264Encoder::new(make_config(64, 64, 30)).unwrap();
    let dbg = format!("{enc:?}");
    assert!(dbg.contains("OpenH264Encoder"));
    assert!(dbg.contains("config"));
}

#[test]
fn test_openh264_encoder_zero_fps_duration() {
    let config = EncoderConfig::with_codec(64, 64, 0, Codec::H264);
    let mut enc = OpenH264Encoder::new(config).unwrap();
    let frame = RawFrame::new(vec![128u8; 64 * 64 * 4], 64, 64, 64 * 4, 500);
    let packet = enc.encode(EncoderInput::Cpu(&frame)).unwrap().unwrap();
    assert_eq!(packet.duration_us, 0);
    assert_eq!(packet.timestamp_us, 500);
}

#[test]
fn test_openh264_encoder_rejects_odd_width() {
    let config = EncoderConfig::with_codec(63, 64, 30, Codec::H264);
    let err = OpenH264Encoder::new(config).unwrap_err();
    assert!(matches!(
        err,
        VideoError::InvalidDimensions {
            width: 63,
            height: 64
        }
    ));
}

#[test]
fn test_openh264_encoder_rejects_odd_height() {
    let config = EncoderConfig::with_codec(64, 63, 30, Codec::H264);
    let err = OpenH264Encoder::new(config).unwrap_err();
    assert!(matches!(
        err,
        VideoError::InvalidDimensions {
            width: 64,
            height: 63
        }
    ));
}

// ── NAL unit parsing tests ─────────────────────────────────────────────

#[test]
fn test_split_h264_nal_units_empty() {
    assert!(split_h264_nal_units(&[]).is_empty());
}

#[test]
fn test_split_h264_nal_units_single_nal() {
    let data = [0x00u8, 0x00, 0x00, 0x01, 0x67, 0x42, 0x00];
    let nals = split_h264_nal_units(&data);
    assert_eq!(nals.len(), 1);
    assert_eq!(nals[0], &[0x67, 0x42, 0x00]);
}

#[test]
fn test_split_h264_nal_units_multiple_nals() {
    let data = [
        0x00u8, 0x00, 0x00, 0x01, 0x67, 0x42, // SPS
        0x00, 0x00, 0x00, 0x01, 0x68, 0x01, // PPS
        0x00, 0x00, 0x00, 0x01, 0x65, 0x88, // IDR slice
    ];
    let nals = split_h264_nal_units(&data);
    assert_eq!(nals.len(), 3);
    assert_eq!(nals[0], &[0x67, 0x42]);
    assert_eq!(nals[1], &[0x68, 0x01]);
    assert_eq!(nals[2], &[0x65, 0x88]);
}

#[test]
fn test_split_h264_nal_units_3byte_start_codes() {
    let data = [
        0x00u8, 0x00, 0x01, 0x67, 0x42, // SPS
        0x00, 0x00, 0x01, 0x65, 0x88, // IDR slice
    ];
    let nals = split_h264_nal_units(&data);
    assert_eq!(nals.len(), 2);
    assert_eq!(nals[0], &[0x67, 0x42]);
    assert_eq!(nals[1], &[0x65, 0x88]);
}

#[test]
fn test_split_h264_nal_units_no_start_codes() {
    let data = [0x67u8, 0x42, 0x00, 0x1f];
    assert!(split_h264_nal_units(&data).is_empty());
}

// ── Keyframe detection tests ───────────────────────────────────────────

#[test]
fn test_is_h264_keyframe_with_idr_slice() {
    let data = [
        0x00u8, 0x00, 0x00, 0x01, 0x67, 0x42, // SPS (NAL type 7)
        0x00, 0x00, 0x00, 0x01, 0x68, 0x01, // PPS (NAL type 8)
        0x00, 0x00, 0x00, 0x01, 0x65, 0x88, // IDR slice (NAL type 5)
    ];
    assert!(is_h264_keyframe(&data));
}

#[test]
fn test_is_h264_keyframe_without_idr_slice() {
    let data = [
        0x00u8, 0x00, 0x00, 0x01, 0x67, 0x42, // SPS (NAL type 7)
        0x00, 0x00, 0x00, 0x01, 0x68, 0x01, // PPS (NAL type 8)
        0x00, 0x00, 0x00, 0x01, 0x41, 0x9a, // P-slice (NAL type 1)
    ];
    assert!(!is_h264_keyframe(&data));
}

#[test]
fn test_is_h264_keyframe_only_idr_slice() {
    let data = [0x00u8, 0x00, 0x00, 0x01, 0x65, 0x88]; // IDR slice (NAL type 5)
    assert!(is_h264_keyframe(&data));
}

#[test]
fn test_is_h264_keyframe_empty_data() {
    assert!(!is_h264_keyframe(&[]));
}

#[test]
fn test_is_h264_keyframe_no_valid_nals() {
    let data = [0x67u8, 0x42, 0x00]; // No start codes
    assert!(!is_h264_keyframe(&data));
}

#[test]
fn test_is_h264_keyframe_empty_nal() {
    let data = [0x00u8, 0x00, 0x00, 0x01]; // Start code but no NAL data
    assert!(!is_h264_keyframe(&data));
}

// ── Integration test: keyframe detection in actual encoding ────────────

#[test]
fn test_openh264_encoder_keyframe_detection() {
    let mut enc = OpenH264Encoder::new(make_config(64, 64, 30)).unwrap();
    let frame = RawFrame::new(vec![128u8; 64 * 64 * 4], 64, 64, 64 * 4, 1000);

    // First frame should be a keyframe
    let result = enc.encode(EncoderInput::Cpu(&frame)).unwrap();
    assert!(result.is_some());
    let packet = result.unwrap();

    // Verify that keyframe detection works on the actual encoded data
    let is_keyframe_detected = is_h264_keyframe(&packet.data);
    assert_eq!(
        packet.is_keyframe, is_keyframe_detected,
        "EncodedPacket.is_keyframe should match actual bitstream analysis"
    );
}

// ── Keyframe interval (GOP) tests ─────────────────────────────────────

#[test]
fn test_openh264_encoder_keyframe_interval_at_60fps() {
    let enc = OpenH264Encoder::new(make_config(64, 64, 60)).unwrap();
    // 0.5 seconds at 60fps = 30 frames
    assert_eq!(enc.keyframe_interval, 30);
}

#[test]
fn test_openh264_encoder_keyframe_interval_at_30fps() {
    let enc = OpenH264Encoder::new(make_config(64, 64, 30)).unwrap();
    // 0.5 seconds at 30fps = 15 frames
    assert_eq!(enc.keyframe_interval, 15);
}

#[test]
fn test_openh264_encoder_keyframe_interval_at_1fps() {
    let enc = OpenH264Encoder::new(make_config(64, 64, 1)).unwrap();
    // fps/2 = 0, clamped to 1
    assert_eq!(enc.keyframe_interval, 1);
}

#[test]
fn test_openh264_encoder_keyframe_interval_at_0fps() {
    let enc = OpenH264Encoder::new(make_config(64, 64, 0)).unwrap();
    // fps/2 = 0, clamped to 1
    assert_eq!(enc.keyframe_interval, 1);
}

#[test]
fn test_openh264_encoder_keyframe_interval_at_2fps() {
    let enc = OpenH264Encoder::new(make_config(64, 64, 2)).unwrap();
    // 0.5 seconds at 2fps = 1 frame
    assert_eq!(enc.keyframe_interval, 1);
}

#[test]
fn test_openh264_encoder_frame_index_starts_at_zero() {
    let enc = OpenH264Encoder::new(make_config(64, 64, 30)).unwrap();
    assert_eq!(enc.frame_index, 0);
}

#[test]
fn test_openh264_encoder_frame_index_increments_on_encode() {
    let mut enc = OpenH264Encoder::new(make_config(64, 64, 30)).unwrap();
    let frame = RawFrame::new(vec![128u8; 64 * 64 * 4], 64, 64, 64 * 4, 0);

    enc.encode(EncoderInput::Cpu(&frame)).unwrap();
    assert_eq!(enc.frame_index, 1);

    enc.encode(EncoderInput::Cpu(&frame)).unwrap();
    assert_eq!(enc.frame_index, 2);
}

#[test]
fn test_openh264_encoder_produces_keyframe_at_interval() {
    // Use 4fps so keyframe_interval = 2 (every 2 frames)
    let mut enc = OpenH264Encoder::new(make_config(64, 64, 4)).unwrap();
    assert_eq!(enc.keyframe_interval, 2);

    // Encode enough frames to trigger forced keyframes
    let mut keyframe_indices = Vec::new();
    for i in 0..6 {
        let f = RawFrame::new(vec![128u8; 64 * 64 * 4], 64, 64, 64 * 4, i * 250_000);
        if let Some(pkt) = enc.encode(EncoderInput::Cpu(&f)).unwrap()
            && pkt.is_keyframe
        {
            keyframe_indices.push(i);
        }
    }

    // Frame 0 should be a keyframe (first frame is always IDR)
    assert!(
        keyframe_indices.contains(&0),
        "First frame should be a keyframe, got keyframes at: {keyframe_indices:?}"
    );
    // Frame 2 and/or 4 should also be keyframes (forced every 2 frames)
    assert!(
        keyframe_indices.len() >= 2,
        "Should have at least 2 keyframes in 6 frames with interval=2, got: {keyframe_indices:?}"
    );
}

#[test]
fn test_openh264_encoder_first_frame_does_not_force_idr() {
    // force_intra_frame is only called when frame_index > 0, so the
    // first frame relies on the encoder's natural IDR behavior.
    let mut enc = OpenH264Encoder::new(make_config(64, 64, 30)).unwrap();
    let frame = RawFrame::new(vec![128u8; 64 * 64 * 4], 64, 64, 64 * 4, 0);

    let pkt = enc.encode(EncoderInput::Cpu(&frame)).unwrap().unwrap();
    // First frame is naturally a keyframe from the encoder
    assert!(pkt.is_keyframe);
    assert_eq!(enc.frame_index, 1);
}

#[test]
fn test_openh264_encoder_duration_us_at_60fps() {
    let mut enc = OpenH264Encoder::new(make_config(64, 64, 60)).unwrap();
    let frame = RawFrame::new(vec![128u8; 64 * 64 * 4], 64, 64, 64 * 4, 0);
    let pkt = enc.encode(EncoderInput::Cpu(&frame)).unwrap().unwrap();
    assert_eq!(pkt.duration_us, 1_000_000 / 60);
}
