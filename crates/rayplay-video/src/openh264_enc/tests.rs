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
