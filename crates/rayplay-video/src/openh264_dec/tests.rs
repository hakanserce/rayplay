use super::*;

#[test]
fn test_openh264_decoder_new_creates_session() {
    let dec = OpenH264Decoder::new(Codec::H264);
    assert!(dec.is_ok());
}

#[test]
fn test_openh264_decoder_rejects_hevc() {
    let err = OpenH264Decoder::new(Codec::Hevc).unwrap_err();
    assert!(matches!(err, VideoError::UnsupportedCodec { .. }));
}

#[test]
fn test_openh264_decoder_codec_accessor() {
    let dec = OpenH264Decoder::new(Codec::H264).unwrap();
    assert_eq!(dec.codec(), Codec::H264);
}

#[test]
fn test_openh264_decoder_flush_returns_empty() {
    let mut dec = OpenH264Decoder::new(Codec::H264).unwrap();
    assert!(dec.flush().unwrap().is_empty());
}

#[test]
fn test_openh264_decoder_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<OpenH264Decoder>();
}

#[test]
fn test_openh264_round_trip_encode_decode() {
    use crate::encoder::{EncoderConfig, EncoderInput, VideoEncoder};
    use crate::frame::RawFrame;
    use crate::openh264_enc::OpenH264Encoder;

    let config = EncoderConfig::with_codec(64, 64, 30, Codec::H264);
    let mut encoder = OpenH264Encoder::new(config).unwrap();

    // Create a frame with non-zero pixel data
    let mut data = vec![0u8; 64 * 64 * 4];
    for pixel in data.chunks_exact_mut(4) {
        pixel[0] = 100; // B
        pixel[1] = 150; // G
        pixel[2] = 200; // R
        pixel[3] = 255; // A
    }
    let frame = RawFrame::new(data, 64, 64, 64 * 4, 42_000);

    let packet = encoder
        .encode(EncoderInput::Cpu(&frame))
        .unwrap()
        .expect("encoder should produce a packet");

    let mut decoder = OpenH264Decoder::new(Codec::H264).unwrap();
    let decoded_frame = decoder
        .decode(&packet)
        .unwrap()
        .expect("decoder should produce a frame for the first keyframe");

    assert_eq!(decoded_frame.width, 64);
    assert_eq!(decoded_frame.height, 64);
    assert_eq!(decoded_frame.format, PixelFormat::Bgra8);
    assert_eq!(decoded_frame.timestamp_us, 42_000);
    assert!(!decoded_frame.data.is_empty());
    assert!(
        decoded_frame.data.iter().any(|&b| b != 0),
        "decoded frame should have non-zero pixel data"
    );
}

#[test]
fn test_yuv_to_bgra_pure_black() {
    use openh264::formats::YUVSlices;
    // Y=0, U=128, V=128 → R=0, G=0, B=0
    let y = vec![0u8; 4];
    let u = vec![128u8; 1];
    let v = vec![128u8; 1];
    let slices = YUVSlices::new((&y, &u, &v), (2, 2), (2, 1, 1));
    let bgra = yuv_to_bgra(&slices);
    assert_eq!(bgra.len(), 2 * 2 * 4);
    for pixel in bgra.chunks_exact(4) {
        assert!(pixel[0] < 5, "B should be near 0: {}", pixel[0]);
        assert!(pixel[1] < 5, "G should be near 0: {}", pixel[1]);
        assert!(pixel[2] < 5, "R should be near 0: {}", pixel[2]);
        assert_eq!(pixel[3], 255, "A should be 255");
    }
}

#[test]
fn test_yuv_to_bgra_pure_white() {
    use openh264::formats::YUVSlices;
    // Y=255, U=128, V=128 → R≈255, G≈255, B≈255
    let y = vec![255u8; 4];
    let u = vec![128u8; 1];
    let v = vec![128u8; 1];
    let slices = YUVSlices::new((&y, &u, &v), (2, 2), (2, 1, 1));
    let bgra = yuv_to_bgra(&slices);
    for pixel in bgra.chunks_exact(4) {
        assert!(pixel[0] > 250, "B should be near 255: {}", pixel[0]);
        assert!(pixel[1] > 250, "G should be near 255: {}", pixel[1]);
        assert!(pixel[2] > 250, "R should be near 255: {}", pixel[2]);
        assert_eq!(pixel[3], 255);
    }
}

#[test]
fn test_openh264_decoder_debug_impl() {
    let dec = OpenH264Decoder::new(Codec::H264).unwrap();
    let dbg = format!("{dec:?}");
    assert!(dbg.contains("OpenH264Decoder"));
    assert!(dbg.contains("H264"));
}

#[test]
fn test_openh264_decoder_returns_none_for_parameter_sets_only() {
    use crate::encoder::{EncoderConfig, EncoderInput, VideoEncoder};
    use crate::frame::RawFrame;
    use crate::openh264_enc::OpenH264Encoder;

    // Encode a real frame to get a valid bitstream with SPS/PPS + slice NALs.
    let config = EncoderConfig::with_codec(64, 64, 30, Codec::H264);
    let mut encoder = OpenH264Encoder::new(config).unwrap();
    let frame = RawFrame::new(vec![128u8; 64 * 64 * 4], 64, 64, 64 * 4, 0);
    let packet = encoder
        .encode(EncoderInput::Cpu(&frame))
        .unwrap()
        .expect("packet");

    // Extract only the SPS NAL (starts at byte 0, type 0x67).
    // Walk Annex-B start codes to find just the SPS.
    let data = &packet.data;
    let mut nal_starts = vec![];
    for i in 0..data.len().saturating_sub(3) {
        if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 0 && data[i + 3] == 1 {
            nal_starts.push(i);
        }
    }

    // Feed only the first NAL (SPS) — decoder should buffer and return None.
    if nal_starts.len() >= 2 {
        let sps_only = &data[nal_starts[0]..nal_starts[1]];
        let sps_packet = EncodedPacket::new(sps_only.to_vec(), false, 0, 0);
        let mut dec = OpenH264Decoder::new(Codec::H264).unwrap();
        let result = dec.decode(&sps_packet).unwrap();
        assert!(
            result.is_none(),
            "SPS-only packet should not produce a decoded frame"
        );
    }
}

#[test]
fn test_yuv_to_bgra_extreme_high_v_clamps_r_to_max() {
    use openh264::formats::YUVSlices;
    // Y=255, U=0, V=255 → R exceeds 255 before clamping (exercises upper clamp)
    let y = vec![255u8; 4];
    let u = vec![0u8; 1];
    let v = vec![255u8; 1];
    let slices = YUVSlices::new((&y, &u, &v), (2, 2), (2, 1, 1));
    let bgra = yuv_to_bgra(&slices);
    assert_eq!(bgra.len(), 2 * 2 * 4);
    assert_eq!(bgra[2], 255, "R should be clamped to 255");
    assert_eq!(bgra[3], 255);
}

#[test]
fn test_yuv_to_bgra_extreme_uv_clamps_g_to_zero() {
    use openh264::formats::YUVSlices;
    // Y=0, U=255, V=255 → G goes deeply negative before clamping
    let y = vec![0u8; 4];
    let u = vec![255u8; 1];
    let v = vec![255u8; 1];
    let slices = YUVSlices::new((&y, &u, &v), (2, 2), (2, 1, 1));
    let bgra = yuv_to_bgra(&slices);
    assert_eq!(bgra.len(), 2 * 2 * 4);
    assert_eq!(bgra[1], 0, "G should be clamped to 0");
    assert_eq!(bgra[3], 255);
}

#[test]
fn test_yuv_to_bgra_mid_gray() {
    use openh264::formats::YUVSlices;
    // Y=128, U=128, V=128 → mid-gray
    let y = vec![128u8; 4];
    let u = vec![128u8; 1];
    let v = vec![128u8; 1];
    let slices = YUVSlices::new((&y, &u, &v), (2, 2), (2, 1, 1));
    let bgra = yuv_to_bgra(&slices);
    for pixel in bgra.chunks_exact(4) {
        // Should be around 128 for all channels
        assert!(pixel[0] > 100 && pixel[0] < 160, "B={}", pixel[0]);
        assert!(pixel[1] > 100 && pixel[1] < 160, "G={}", pixel[1]);
        assert!(pixel[2] > 100 && pixel[2] < 160, "R={}", pixel[2]);
        assert_eq!(pixel[3], 255);
    }
}

#[test]
fn test_openh264_decoder_decode_invalid_data_returns_error_or_none() {
    let mut dec = OpenH264Decoder::new(Codec::H264).unwrap();
    // Feed garbage data — decoder may error or return None
    let packet = EncodedPacket::new(vec![0xFF; 64], false, 0, 16_667);
    let result = dec.decode(&packet);
    // Either Ok(None) or Err — both are acceptable for invalid data
    if let Ok(Some(_)) = result {
        panic!("should not decode garbage into a frame");
    }
}
