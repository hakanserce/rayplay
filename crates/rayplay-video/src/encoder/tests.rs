use super::*;

// ── EncoderConfig ──────────────────────────────────────────────────────────

#[test]
fn test_encoder_config_new_defaults() {
    let cfg = EncoderConfig::new(1920, 1080, 60);
    assert_eq!(cfg.codec, Codec::Hevc);
    assert_eq!(cfg.width, 1920);
    assert_eq!(cfg.height, 1080);
    assert_eq!(cfg.fps, 60);
    assert_eq!(cfg.bitrate, Bitrate::Auto);
}

#[test]
fn test_encoder_config_with_codec_hevc() {
    let cfg = EncoderConfig::with_codec(1920, 1080, 60, Codec::Hevc);
    assert_eq!(cfg.codec, Codec::Hevc);
    assert_eq!(cfg.width, 1920);
    assert_eq!(cfg.height, 1080);
    assert_eq!(cfg.fps, 60);
    assert_eq!(cfg.bitrate, Bitrate::Auto);
}

#[test]
fn test_encoder_config_with_codec_h264() {
    let cfg = EncoderConfig::with_codec(1280, 720, 30, Codec::H264);
    assert_eq!(cfg.codec, Codec::H264);
    assert_eq!(cfg.width, 1280);
    assert_eq!(cfg.height, 720);
    assert_eq!(cfg.fps, 30);
    assert_eq!(cfg.bitrate, Bitrate::Auto);
}

#[test]
fn test_encoder_config_with_bitrate_override() {
    let cfg = EncoderConfig::new(1920, 1080, 60).with_bitrate(Bitrate::Mbps(8));
    assert_eq!(cfg.bitrate, Bitrate::Mbps(8));
}

#[test]
fn test_encoder_config_chained_methods() {
    let cfg = EncoderConfig::with_codec(1280, 720, 30, Codec::H264).with_bitrate(Bitrate::Mbps(5));
    assert_eq!(cfg.codec, Codec::H264);
    assert_eq!(cfg.bitrate, Bitrate::Mbps(5));
    assert_eq!(cfg.width, 1280);
    assert_eq!(cfg.height, 720);
    assert_eq!(cfg.fps, 30);
}

#[test]
fn test_encoder_config_resolved_bitrate_auto_hevc_1080p60() {
    let cfg = EncoderConfig::new(1920, 1080, 60);
    let bps = cfg.resolved_bitrate();
    // At 1080p60 with HEVC: 1920*1080*60/20 ≈ 6_220_800 bps
    assert!(bps >= 1_000_000, "bitrate below minimum: {bps}");
    assert!(bps <= 100_000_000, "bitrate above maximum: {bps}");
}

#[test]
fn test_encoder_config_resolved_bitrate_auto_h264_1080p60() {
    let cfg = EncoderConfig::with_codec(1920, 1080, 60, Codec::H264);
    let bps = cfg.resolved_bitrate();
    // At 1080p60 with H.264: 1920*1080*60/15 ≈ 8_294_400 bps
    assert!(bps >= 1_000_000, "bitrate below minimum: {bps}");
    assert!(bps <= 100_000_000, "bitrate above maximum: {bps}");

    // H.264 should require higher bitrate than HEVC for same resolution
    let hevc_cfg = EncoderConfig::new(1920, 1080, 60);
    let hevc_bps = hevc_cfg.resolved_bitrate();
    assert!(bps > hevc_bps, "H.264 bitrate should exceed HEVC bitrate");
}

#[test]
fn test_encoder_config_resolved_bitrate_auto_hevc_4k60() {
    let cfg = EncoderConfig::new(3840, 2160, 60);
    let bps = cfg.resolved_bitrate();
    // 4K should produce a higher bitrate than 1080p
    let bps_1080p = EncoderConfig::new(1920, 1080, 60).resolved_bitrate();
    assert!(bps > bps_1080p, "4K bitrate should exceed 1080p bitrate");
}

#[test]
fn test_encoder_config_resolved_bitrate_fixed() {
    let cfg = EncoderConfig::new(1920, 1080, 60).with_bitrate(Bitrate::Mbps(12));
    assert_eq!(cfg.resolved_bitrate(), 12_000_000);
}

// ── Bitrate ────────────────────────────────────────────────────────────────

#[test]
fn test_bitrate_auto_clamped_to_minimum_for_tiny_frame() {
    // Tiny 4x4 frame should still hit the 1 Mbps floor
    let bps = Bitrate::Auto.resolve(Codec::Hevc, 4, 4, 30);
    assert_eq!(bps, 1_000_000);
}

#[test]
fn test_bitrate_auto_clamped_to_maximum_for_huge_frame() {
    // Massive resolution should be capped at 100 Mbps
    let bps = Bitrate::Auto.resolve(Codec::Hevc, 15360, 8640, 240);
    assert_eq!(bps, 100_000_000);
}

#[test]
fn test_bitrate_mbps_converts_correctly() {
    assert_eq!(
        Bitrate::Mbps(20).resolve(Codec::Hevc, 1920, 1080, 60),
        20_000_000
    );
}

#[test]
fn test_bitrate_mbps_saturates_on_overflow() {
    // Very large Mbps value must not panic
    let bps = Bitrate::Mbps(u32::MAX).resolve(Codec::Hevc, 1920, 1080, 60);
    assert!(bps > 0);
}

#[test]
fn test_bitrate_auto_h264_higher_than_hevc() {
    // H.264 should require higher bitrate than HEVC for same resolution
    let hevc_bps = Bitrate::Auto.resolve(Codec::Hevc, 1920, 1080, 60);
    let h264_bps = Bitrate::Auto.resolve(Codec::H264, 1920, 1080, 60);
    assert!(
        h264_bps > hevc_bps,
        "H.264 bitrate should exceed HEVC bitrate"
    );
}

#[test]
fn test_bitrate_auto_h264_720p30_calculation() {
    // Test specific H.264 calculation for 720p30
    let bps = Bitrate::Auto.resolve(Codec::H264, 1280, 720, 30);
    let expected = 1280 * 720 * 30 / 15; // H.264 compression factor is 15
    assert_eq!(bps, expected);
}

#[test]
fn test_bitrate_auto_hevc_720p30_calculation() {
    // Test specific HEVC calculation for 720p30
    let bps = Bitrate::Auto.resolve(Codec::Hevc, 1280, 720, 30);
    let expected = 1280 * 720 * 30 / 20; // HEVC compression factor is 20
    assert_eq!(bps, expected);
}

// ── VideoError ─────────────────────────────────────────────────────────────

#[test]
fn test_video_error_not_initialized_message() {
    let msg = VideoError::NotInitialized.to_string();
    assert!(msg.contains("not initialized"));
}

#[test]
fn test_video_error_unsupported_codec_hevc_message() {
    let msg = VideoError::UnsupportedCodec { codec: Codec::Hevc }.to_string();
    assert!(msg.contains("Hevc"));
}

#[test]
fn test_video_error_unsupported_codec_h264_message() {
    let msg = VideoError::UnsupportedCodec { codec: Codec::H264 }.to_string();
    assert!(msg.contains("H264"));
}

#[test]
fn test_video_error_invalid_dimensions_message() {
    let msg = VideoError::InvalidDimensions {
        width: 0,
        height: 0,
    }
    .to_string();
    assert!(msg.contains('0'));
}

#[test]
fn test_video_error_encoding_failed_message() {
    let msg = VideoError::EncodingFailed {
        reason: "test".to_string(),
    }
    .to_string();
    assert!(msg.contains("test"));
}

#[test]
fn test_video_error_decoding_failed_message() {
    let msg = VideoError::DecodingFailed {
        reason: "bad session".to_string(),
    }
    .to_string();
    assert!(msg.contains("bad session"));
}

#[test]
fn test_video_error_corrupt_packet_message() {
    let msg = VideoError::CorruptPacket {
        reason: "truncated NAL".to_string(),
    }
    .to_string();
    assert!(msg.contains("truncated NAL"));
}

// ── compute_auto_bitrate (private, tested via Bitrate::Auto) ──────────────

#[test]
fn test_video_error_unsupported_platform_display() {
    let msg = VideoError::UnsupportedPlatform.to_string();
    assert!(msg.contains("not supported"));
}

#[cfg(all(
    not(target_os = "windows"),
    feature = "fallback",
    not(feature = "ffmpeg-fallback")
))]
#[test]
fn test_create_encoder_auto_returns_openh264_on_non_windows_with_fallback() {
    let result = create_encoder(EncoderConfig::new(1920, 1080, 60), PipelineMode::Auto);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().config().codec, Codec::H264);
}

#[cfg(all(
    not(target_os = "windows"),
    not(feature = "fallback"),
    not(feature = "ffmpeg-fallback")
))]
#[test]
fn test_create_encoder_auto_unsupported_on_non_windows_without_fallback() {
    let result = create_encoder(EncoderConfig::new(1920, 1080, 60), PipelineMode::Auto);
    assert!(matches!(result, Err(VideoError::UnsupportedPlatform)));
}

#[cfg(all(feature = "fallback", not(feature = "ffmpeg-fallback")))]
#[test]
fn test_create_encoder_software_returns_openh264() {
    let result = create_encoder(EncoderConfig::new(1920, 1080, 60), PipelineMode::Software);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().config().codec, Codec::H264);
}

#[cfg(not(any(feature = "fallback", feature = "ffmpeg-fallback")))]
#[test]
fn test_create_encoder_software_unsupported_without_fallback() {
    let result = create_encoder(EncoderConfig::new(1920, 1080, 60), PipelineMode::Software);
    assert!(matches!(result, Err(VideoError::UnsupportedPlatform)));
}

#[cfg(feature = "ffmpeg-fallback")]
#[test]
fn test_create_encoder_software_returns_ffmpeg() {
    let result = create_encoder(EncoderConfig::new(1920, 1080, 60), PipelineMode::Software);
    assert!(result.is_ok());
    // FFmpeg preserves the requested codec (HEVC) unlike OpenH264 which forces H264
    assert_eq!(result.unwrap().config().codec, Codec::Hevc);
}

#[cfg(all(not(target_os = "windows"), feature = "ffmpeg-fallback"))]
#[test]
fn test_create_encoder_auto_returns_ffmpeg_on_non_windows() {
    let result = create_encoder(EncoderConfig::new(1920, 1080, 60), PipelineMode::Auto);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().config().codec, Codec::Hevc);
}

#[test]
fn test_auto_bitrate_scales_with_fps() {
    let bps_60 = Bitrate::Auto.resolve(Codec::Hevc, 1920, 1080, 60);
    let bps_30 = Bitrate::Auto.resolve(Codec::Hevc, 1920, 1080, 30);
    assert!(bps_60 > bps_30, "60fps bitrate should exceed 30fps");
}

// ── Codec enum ─────────────────────────────────────────────────────────────

#[test]
fn test_codec_hevc_debug() {
    let dbg = format!("{:?}", Codec::Hevc);
    assert!(dbg.contains("Hevc"));
}

#[test]
fn test_codec_h264_debug() {
    let dbg = format!("{:?}", Codec::H264);
    assert!(dbg.contains("H264"));
}

#[test]
fn test_codec_hevc_equality() {
    assert_eq!(Codec::Hevc, Codec::Hevc);
    assert_ne!(Codec::Hevc, Codec::H264);
}

#[test]
fn test_codec_h264_equality() {
    assert_eq!(Codec::H264, Codec::H264);
    assert_ne!(Codec::H264, Codec::Hevc);
}

#[test]
fn test_codec_hevc_clone() {
    let codec = Codec::Hevc;
    let cloned = codec.clone();
    assert_eq!(codec, cloned);
}

#[test]
fn test_codec_h264_clone() {
    let codec = Codec::H264;
    let cloned = codec.clone();
    assert_eq!(codec, cloned);
}

// ── EncoderInput ──────────────────────────────────────────────────────────

#[test]
fn test_encoder_input_cpu_construction_and_access() {
    let frame = RawFrame::new(vec![0u8; 16], 2, 2, 8, 42);
    let input = EncoderInput::Cpu(&frame);
    match input {
        EncoderInput::Cpu(f) => {
            assert_eq!(f.width, 2);
            assert_eq!(f.timestamp_us, 42);
        }
        EncoderInput::GpuTexture { .. } => panic!("expected Cpu variant"),
    }
}

#[test]
fn test_encoder_input_gpu_texture_construction_with_null_pointer() {
    let input = EncoderInput::GpuTexture {
        handle: GpuTextureHandle(std::ptr::null_mut()),
        width: 1920,
        height: 1080,
        timestamp_us: 100,
    };
    match input {
        EncoderInput::GpuTexture {
            handle,
            width,
            height,
            timestamp_us,
        } => {
            assert!(handle.0.is_null());
            assert_eq!(width, 1920);
            assert_eq!(height, 1080);
            assert_eq!(timestamp_us, 100);
        }
        EncoderInput::Cpu(_) => panic!("expected GpuTexture variant"),
    }
}

#[test]
fn test_encoder_input_debug_cpu_variant() {
    let frame = RawFrame::new(vec![0u8; 4], 1, 1, 4, 0);
    let input = EncoderInput::Cpu(&frame);
    let dbg = format!("{input:?}");
    assert!(dbg.contains("Cpu"));
}

#[test]
fn test_encoder_input_debug_gpu_texture_variant() {
    let input = EncoderInput::GpuTexture {
        handle: GpuTextureHandle(std::ptr::null_mut()),
        width: 3840,
        height: 2160,
        timestamp_us: 999,
    };
    let dbg = format!("{input:?}");
    assert!(dbg.contains("GpuTexture"));
    assert!(dbg.contains("3840"));
    assert!(dbg.contains("2160"));
    assert!(dbg.contains("999"));
}

#[test]
fn test_encoder_input_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<EncoderInput<'_>>();
}

#[test]
fn test_gpu_texture_handle_debug_shows_pointer() {
    let h = GpuTextureHandle(std::ptr::null_mut());
    let dbg = format!("{h:?}");
    assert!(dbg.contains("GpuTextureHandle"));
}

#[test]
fn test_gpu_texture_handle_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<GpuTextureHandle>();
}

#[test]
fn test_null_encoder_rejects_gpu_texture_input() {
    let config = EncoderConfig::new(1920, 1080, 60);
    let mut enc = NullEncoder {
        config,
        return_packet: true,
    };
    let input = EncoderInput::GpuTexture {
        handle: GpuTextureHandle(std::ptr::null_mut()),
        width: 1920,
        height: 1080,
        timestamp_us: 0,
    };
    let err = enc.encode(input).unwrap_err();
    assert!(matches!(err, VideoError::EncodingFailed { .. }));
}

// ── NullEncoder (test double) ──────────────────────────────────────────────

struct NullEncoder {
    config: EncoderConfig,
    return_packet: bool,
}

impl VideoEncoder for NullEncoder {
    fn encode(&mut self, input: EncoderInput<'_>) -> Result<Option<EncodedPacket>, VideoError> {
        let frame = match input {
            EncoderInput::Cpu(f) => f,
            EncoderInput::GpuTexture { .. } => {
                return Err(VideoError::EncodingFailed {
                    reason: "NullEncoder does not support GPU textures".to_string(),
                });
            }
        };
        if frame.width != self.config.width || frame.height != self.config.height {
            return Err(VideoError::InvalidDimensions {
                width: frame.width,
                height: frame.height,
            });
        }
        if self.return_packet {
            Ok(Some(EncodedPacket::new(
                vec![0u8; 64],
                true,
                frame.timestamp_us,
                16_667,
            )))
        } else {
            Ok(None)
        }
    }

    fn flush(&mut self) -> Result<Vec<EncodedPacket>, VideoError> {
        Ok(vec![])
    }

    fn config(&self) -> &EncoderConfig {
        &self.config
    }
}

#[test]
fn test_video_encoder_trait_encode_returns_packet() {
    let config = EncoderConfig::new(1920, 1080, 60);
    let mut enc = NullEncoder {
        config: config.clone(),
        return_packet: true,
    };
    let frame = RawFrame::new(vec![0u8; 1920 * 1080 * 4], 1920, 1080, 1920 * 4, 0);
    let result = enc.encode(EncoderInput::Cpu(&frame)).unwrap();
    assert!(result.is_some());
}

#[test]
fn test_video_encoder_trait_encode_returns_none_when_buffering() {
    let config = EncoderConfig::new(1920, 1080, 60);
    let mut enc = NullEncoder {
        config: config.clone(),
        return_packet: false,
    };
    let frame = RawFrame::new(vec![0u8; 1920 * 1080 * 4], 1920, 1080, 1920 * 4, 0);
    let result = enc.encode(EncoderInput::Cpu(&frame)).unwrap();
    assert!(result.is_none());
}

#[test]
fn test_video_encoder_trait_encode_rejects_wrong_dimensions() {
    let config = EncoderConfig::new(1920, 1080, 60);
    let mut enc = NullEncoder {
        config,
        return_packet: false,
    };
    let wrong_frame = RawFrame::new(vec![0u8; 4], 2, 2, 8, 0);
    let err = enc.encode(EncoderInput::Cpu(&wrong_frame)).unwrap_err();
    assert!(matches!(err, VideoError::InvalidDimensions { .. }));
}

#[test]
fn test_video_encoder_trait_flush_returns_empty() {
    let config = EncoderConfig::new(1920, 1080, 60);
    let mut enc = NullEncoder {
        config,
        return_packet: false,
    };
    let packets = enc.flush().unwrap();
    assert!(packets.is_empty());
}

#[test]
fn test_video_encoder_trait_config_accessor() {
    let config = EncoderConfig::new(3840, 2160, 60);
    let enc = NullEncoder {
        config: config.clone(),
        return_packet: false,
    };
    assert_eq!(enc.config().width, 3840);
    assert_eq!(enc.config().height, 2160);
}

#[test]
fn test_video_encoder_trait_config_h264() {
    let config = EncoderConfig::with_codec(1920, 1080, 60, Codec::H264);
    let enc = NullEncoder {
        config: config.clone(),
        return_packet: false,
    };
    assert_eq!(enc.config().codec, Codec::H264);
    assert_eq!(enc.config().width, 1920);
    assert_eq!(enc.config().height, 1080);
    assert_eq!(enc.config().fps, 60);
}
