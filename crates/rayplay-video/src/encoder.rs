use thiserror::Error;

use crate::{frame::RawFrame, packet::EncodedPacket};

/// Supported video codecs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Codec {
    /// H.265 / HEVC — default codec, hardware-accelerated on Nvidia RTX 2060+.
    Hevc,
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
    pub fn resolve(&self, width: u32, height: u32, fps: u32) -> u32 {
        match self {
            Self::Auto => compute_auto_bitrate(width, height, fps),
            Self::Mbps(mbps) => mbps.saturating_mul(1_000_000),
        }
    }
}

/// Computes a HEVC bitrate heuristic from resolution and frame rate.
///
/// Targets approximately 6 Mbps at 1080p60 and 25 Mbps at 4K60,
/// clamped between 1 Mbps and 100 Mbps.
///
/// Formula: `pixels * fps / COMPRESSION_FACTOR`, where the factor is
/// tuned so that 1920×1080×60 / 20 ≈ 6 220 800 bps (~6 Mbps).
fn compute_auto_bitrate(width: u32, height: u32, fps: u32) -> u32 {
    // HEVC encodes ~20× more efficiently than raw pixel throughput (empirical).
    const COMPRESSION_FACTOR: u64 = 20;
    let pixels = u64::from(width) * u64::from(height);
    let bps = pixels * u64::from(fps) / COMPRESSION_FACTOR;
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

    /// Overrides the bitrate setting.
    #[must_use]
    pub fn with_bitrate(mut self, bitrate: Bitrate) -> Self {
        self.bitrate = bitrate;
        self
    }

    /// Returns the resolved bitrate in bits per second.
    #[must_use]
    pub fn resolved_bitrate(&self) -> u32 {
        self.bitrate.resolve(self.width, self.height, self.fps)
    }
}

/// Errors produced by video encoder operations.
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
    fn encode(&mut self, frame: &RawFrame) -> Result<Option<EncodedPacket>, VideoError>;

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
    fn test_encoder_config_with_bitrate_override() {
        let cfg = EncoderConfig::new(1920, 1080, 60).with_bitrate(Bitrate::Mbps(8));
        assert_eq!(cfg.bitrate, Bitrate::Mbps(8));
    }

    #[test]
    fn test_encoder_config_resolved_bitrate_auto_1080p60() {
        let cfg = EncoderConfig::new(1920, 1080, 60);
        let bps = cfg.resolved_bitrate();
        // At 1080p60: 1920*1080*60/1500 ≈ 8_294_400 bps
        assert!(bps >= 1_000_000, "bitrate below minimum: {bps}");
        assert!(bps <= 100_000_000, "bitrate above maximum: {bps}");
    }

    #[test]
    fn test_encoder_config_resolved_bitrate_auto_4k60() {
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
        let bps = Bitrate::Auto.resolve(4, 4, 30);
        assert_eq!(bps, 1_000_000);
    }

    #[test]
    fn test_bitrate_auto_clamped_to_maximum_for_huge_frame() {
        // Massive resolution should be capped at 100 Mbps
        let bps = Bitrate::Auto.resolve(15360, 8640, 240);
        assert_eq!(bps, 100_000_000);
    }

    #[test]
    fn test_bitrate_mbps_converts_correctly() {
        assert_eq!(Bitrate::Mbps(20).resolve(1920, 1080, 60), 20_000_000);
    }

    #[test]
    fn test_bitrate_mbps_saturates_on_overflow() {
        // Very large Mbps value must not panic
        let bps = Bitrate::Mbps(u32::MAX).resolve(1920, 1080, 60);
        assert!(bps > 0);
    }

    // ── VideoError ─────────────────────────────────────────────────────────────

    #[test]
    fn test_video_error_not_initialized_message() {
        let msg = VideoError::NotInitialized.to_string();
        assert!(msg.contains("not initialized"));
    }

    #[test]
    fn test_video_error_unsupported_codec_message() {
        let msg = VideoError::UnsupportedCodec { codec: Codec::Hevc }.to_string();
        assert!(msg.contains("Hevc"));
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

    // ── compute_auto_bitrate (private, tested via Bitrate::Auto) ──────────────

    #[test]
    fn test_auto_bitrate_scales_with_fps() {
        let bps_60 = Bitrate::Auto.resolve(1920, 1080, 60);
        let bps_30 = Bitrate::Auto.resolve(1920, 1080, 30);
        assert!(bps_60 > bps_30, "60fps bitrate should exceed 30fps");
    }

    // ── NullEncoder (test double) ──────────────────────────────────────────────

    struct NullEncoder {
        config: EncoderConfig,
        return_packet: bool,
    }

    impl VideoEncoder for NullEncoder {
        fn encode(&mut self, frame: &RawFrame) -> Result<Option<EncodedPacket>, VideoError> {
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
        let result = enc.encode(&frame).unwrap();
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
        let result = enc.encode(&frame).unwrap();
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
        let err = enc.encode(&wrong_frame).unwrap_err();
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
}
