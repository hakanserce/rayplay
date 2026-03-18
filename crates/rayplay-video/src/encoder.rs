use std::fmt;

use thiserror::Error;

use crate::{frame::RawFrame, packet::EncodedPacket};

/// Opaque handle to a GPU-resident texture for zero-copy encoding.
///
/// On Windows, wraps a `*mut ID3D11Texture2D` COM pointer transferred from
/// [`ZeroCopyCapturer::acquire_texture`].  The resource must remain valid
/// (i.e. the DXGI frame must not be released) until the encoder has consumed
/// it and [`ZeroCopyCapturer::release_frame`] has been called.
#[repr(transparent)]
pub struct GpuTextureHandle(pub *mut std::ffi::c_void);

// SAFETY: only accessed on the single encoding thread that owns the D3D11 context.
unsafe impl Send for GpuTextureHandle {}

impl fmt::Debug for GpuTextureHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GpuTextureHandle({:p})", self.0)
    }
}

/// Input for a video encoder — CPU pixels or GPU-resident texture.
pub enum EncoderInput<'a> {
    /// CPU-accessible raw pixel data (ADR-001 Option A fallback).
    Cpu(&'a RawFrame),
    /// Opaque GPU texture handle (zero-copy ADR-001 Option B).
    GpuTexture {
        handle: GpuTextureHandle,
        width: u32,
        height: u32,
        timestamp_us: u64,
    },
}

// SAFETY: `GpuTexture::handle` is only dereferenced on the encoding thread that
// owns the D3D11 device context — the same single-thread model as `DxgiCapture`.
unsafe impl Send for EncoderInput<'_> {}

impl fmt::Debug for EncoderInput<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cpu(frame) => f.debug_tuple("Cpu").field(frame).finish(),
            Self::GpuTexture {
                handle,
                width,
                height,
                timestamp_us,
            } => f
                .debug_struct("GpuTexture")
                .field("handle", handle)
                .field("width", width)
                .field("height", height)
                .field("timestamp_us", timestamp_us)
                .finish(),
        }
    }
}

/// Supported video codecs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Codec {
    /// H.265 / HEVC — default codec, hardware-accelerated on Nvidia RTX 2060+.
    Hevc,
    /// H.264 / AVC — widely supported codec, hardware-accelerated on most GPUs.
    H264,
}

/// Encoder bitrate setting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bitrate {
    /// Automatically computed from resolution and frame rate.
    Auto,
    /// Fixed bitrate in megabits per second.
    Mbps(u32),
}

impl Bitrate {
    /// Resolves the bitrate to bits per second for the given stream parameters.
    #[must_use]
    pub fn resolve(&self, codec: &Codec, width: u32, height: u32, fps: u32) -> u32 {
        match self {
            Self::Auto => compute_auto_bitrate(codec, width, height, fps),
            Self::Mbps(mbps) => mbps.saturating_mul(1_000_000),
        }
    }
}

/// Computes a codec-aware bitrate heuristic from resolution and frame rate.
///
/// For HEVC: targets approximately 6 Mbps at 1080p60 and 25 Mbps at 4K60.
/// For H.264: targets approximately 8 Mbps at 1080p60 and 33 Mbps at 4K60.
/// Clamped between 1 Mbps and 100 Mbps for both codecs.
///
/// Formula: `pixels * fps / COMPRESSION_FACTOR`, where the factor depends
/// on the codec's compression efficiency.
fn compute_auto_bitrate(codec: &Codec, width: u32, height: u32, fps: u32) -> u32 {
    let compression_factor = match codec {
        Codec::Hevc => 20, // HEVC encodes ~20× more efficiently than raw pixel throughput
        Codec::H264 => 15, // H.264 encodes ~15× more efficiently than raw pixel throughput
    };
    let pixels = u64::from(width) * u64::from(height);
    let bps = pixels * u64::from(fps) / compression_factor;
    bps.clamp(1_000_000, 100_000_000) as u32
}

/// Configuration for a video encoder session.
#[derive(Debug, Clone)]
pub struct EncoderConfig {
    /// Video codec to use for encoding.
    pub codec: Codec,
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Target frame rate.
    pub fps: u32,
    /// Bitrate setting (auto or fixed).
    pub bitrate: Bitrate,
}

impl EncoderConfig {
    /// Creates a new config with HEVC codec and auto bitrate.
    #[must_use]
    pub fn new(width: u32, height: u32, fps: u32) -> Self {
        Self {
            codec: Codec::Hevc,
            width,
            height,
            fps,
            bitrate: Bitrate::Auto,
        }
    }

    /// Creates a new config with the specified codec and auto bitrate.
    #[must_use]
    pub fn with_codec(width: u32, height: u32, fps: u32, codec: Codec) -> Self {
        Self {
            codec,
            width,
            height,
            fps,
            bitrate: Bitrate::Auto,
        }
    }

    /// Overrides the bitrate setting.
    #[must_use]
    pub fn with_bitrate(mut self, bitrate: Bitrate) -> Self {
        self.bitrate = bitrate;
        self
    }

    /// Returns the resolved bitrate in bits per second.
    #[must_use]
    pub fn resolved_bitrate(&self) -> u32 {
        self.bitrate
            .resolve(&self.codec, self.width, self.height, self.fps)
    }
}

/// Errors produced by video encoder and decoder operations.
#[derive(Debug, Error)]
pub enum VideoError {
    #[error("encoder session not initialized")]
    NotInitialized,

    #[error("unsupported codec: {codec:?}")]
    UnsupportedCodec { codec: Codec },

    #[error("invalid frame dimensions: {width}x{height}")]
    InvalidDimensions { width: u32, height: u32 },

    #[error("encoding failed: {reason}")]
    EncodingFailed { reason: String },

    #[error("decoding failed: {reason}")]
    DecodingFailed { reason: String },

    #[error("corrupt packet: {reason}")]
    CorruptPacket { reason: String },

    #[error("video encoding is not supported on this platform")]
    UnsupportedPlatform,
}

/// Returns the platform-appropriate hardware encoder.
///
/// On Windows, returns an [`NvencEncoder`](crate::nvenc::NvencEncoder) backed
/// by NVENC.  On other platforms returns [`VideoError::UnsupportedPlatform`].
///
/// # Errors
///
/// Returns [`VideoError::UnsupportedPlatform`] on non-Windows, or
/// [`VideoError::EncodingFailed`] if the NVENC session cannot be opened.
// On Windows the config is consumed by NvencEncoder::new; the non-Windows branch
// must accept the same signature.
#[allow(clippy::needless_pass_by_value)]
pub fn create_encoder(config: EncoderConfig) -> Result<Box<dyn VideoEncoder>, VideoError> {
    #[cfg(target_os = "windows")]
    {
        use crate::nvenc::NvencEncoder;
        NvencEncoder::new(config).map(|e| Box::new(e) as Box<dyn VideoEncoder>)
    }
    #[cfg(not(target_os = "windows"))]
    {
        #[cfg(feature = "fallback")]
        {
            use crate::openh264_enc::OpenH264Encoder;
            let fallback_config =
                EncoderConfig::with_codec(config.width, config.height, config.fps, Codec::H264)
                    .with_bitrate(config.bitrate.clone());
            OpenH264Encoder::new(fallback_config).map(|e| Box::new(e) as Box<dyn VideoEncoder>)
        }
        #[cfg(not(feature = "fallback"))]
        {
            let _ = config;
            Err(VideoError::UnsupportedPlatform)
        }
    }
}

/// Trait for hardware or software video encoders.
///
/// Implementations must be `Send` so they can be driven from a dedicated
/// encoding thread. The `encode` → `flush` lifecycle mirrors NVENC's
/// asynchronous pipeline.
pub trait VideoEncoder: Send {
    /// Submits a raw frame for encoding.
    ///
    /// Returns `Ok(Some(packet))` when an encoded packet is immediately
    /// available, `Ok(None)` when the encoder is buffering, or an error.
    ///
    /// # Errors
    ///
    /// Returns `VideoError::InvalidDimensions` if the frame size does not
    /// match the encoder configuration, or `VideoError::EncodingFailed` on
    /// an internal encoder error.
    fn encode(&mut self, input: EncoderInput<'_>) -> Result<Option<EncodedPacket>, VideoError>;

    /// Flushes any buffered frames and returns all remaining encoded packets.
    ///
    /// Call this at end-of-stream or before reconfiguring the encoder.
    ///
    /// # Errors
    ///
    /// Returns `VideoError::EncodingFailed` if flushing fails.
    fn flush(&mut self) -> Result<Vec<EncodedPacket>, VideoError>;

    /// Returns the active encoder configuration.
    fn config(&self) -> &EncoderConfig;
}

#[cfg(test)]
mod tests {
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
        let cfg =
            EncoderConfig::with_codec(1280, 720, 30, Codec::H264).with_bitrate(Bitrate::Mbps(5));
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
        let bps = Bitrate::Auto.resolve(&Codec::Hevc, 4, 4, 30);
        assert_eq!(bps, 1_000_000);
    }

    #[test]
    fn test_bitrate_auto_clamped_to_maximum_for_huge_frame() {
        // Massive resolution should be capped at 100 Mbps
        let bps = Bitrate::Auto.resolve(&Codec::Hevc, 15360, 8640, 240);
        assert_eq!(bps, 100_000_000);
    }

    #[test]
    fn test_bitrate_mbps_converts_correctly() {
        assert_eq!(
            Bitrate::Mbps(20).resolve(&Codec::Hevc, 1920, 1080, 60),
            20_000_000
        );
    }

    #[test]
    fn test_bitrate_mbps_saturates_on_overflow() {
        // Very large Mbps value must not panic
        let bps = Bitrate::Mbps(u32::MAX).resolve(&Codec::Hevc, 1920, 1080, 60);
        assert!(bps > 0);
    }

    #[test]
    fn test_bitrate_auto_h264_higher_than_hevc() {
        // H.264 should require higher bitrate than HEVC for same resolution
        let hevc_bps = Bitrate::Auto.resolve(&Codec::Hevc, 1920, 1080, 60);
        let h264_bps = Bitrate::Auto.resolve(&Codec::H264, 1920, 1080, 60);
        assert!(
            h264_bps > hevc_bps,
            "H.264 bitrate should exceed HEVC bitrate"
        );
    }

    #[test]
    fn test_bitrate_auto_h264_720p30_calculation() {
        // Test specific H.264 calculation for 720p30
        let bps = Bitrate::Auto.resolve(&Codec::H264, 1280, 720, 30);
        let expected = 1280 * 720 * 30 / 15; // H.264 compression factor is 15
        assert_eq!(bps, expected);
    }

    #[test]
    fn test_bitrate_auto_hevc_720p30_calculation() {
        // Test specific HEVC calculation for 720p30
        let bps = Bitrate::Auto.resolve(&Codec::Hevc, 1280, 720, 30);
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

    #[cfg(all(not(target_os = "windows"), feature = "fallback"))]
    #[test]
    fn test_create_encoder_returns_openh264_on_non_windows_with_fallback() {
        let result = create_encoder(EncoderConfig::new(1920, 1080, 60));
        assert!(result.is_ok());
        // create_encoder forces H264 for the fallback path
        assert_eq!(result.unwrap().config().codec, Codec::H264);
    }

    #[cfg(all(not(target_os = "windows"), not(feature = "fallback")))]
    #[test]
    fn test_create_encoder_unsupported_on_non_windows_without_fallback() {
        let result = create_encoder(EncoderConfig::new(1920, 1080, 60));
        assert!(matches!(result, Err(VideoError::UnsupportedPlatform)));
    }

    #[test]
    fn test_auto_bitrate_scales_with_fps() {
        let bps_60 = Bitrate::Auto.resolve(&Codec::Hevc, 1920, 1080, 60);
        let bps_30 = Bitrate::Auto.resolve(&Codec::Hevc, 1920, 1080, 30);
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
}
