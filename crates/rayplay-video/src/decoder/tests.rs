use super::*;
use crate::{decoded_frame::PixelFormat, packet::EncodedPacket};

// ── NullDecoder test double ────────────────────────────────────────────────

struct NullDecoder {
    codec: Codec,
    emit_frame: bool,
    fail_on_corrupt: bool,
}

impl VideoDecoder for NullDecoder {
    fn decode(&mut self, packet: &EncodedPacket) -> Result<Option<DecodedFrame>, VideoError> {
        if self.fail_on_corrupt && packet.data.is_empty() {
            return Err(VideoError::CorruptPacket {
                reason: "empty packet".to_string(),
            });
        }
        if self.emit_frame {
            Ok(Some(DecodedFrame::new_cpu(
                vec![0u8; 1920 * 1080 * 4],
                1920,
                1080,
                1920 * 4,
                PixelFormat::Bgra8,
                packet.timestamp_us,
            )))
        } else {
            Ok(None)
        }
    }

    fn flush(&mut self) -> Result<Vec<DecodedFrame>, VideoError> {
        Ok(vec![])
    }

    fn codec(&self) -> Codec {
        self.codec
    }
}

// ── create_decoder factory ─────────────────────────────────────────────────

#[cfg(all(
    not(target_os = "macos"),
    feature = "fallback",
    not(feature = "ffmpeg-fallback")
))]
#[test]
fn test_create_decoder_auto_returns_openh264_on_non_macos_with_fallback() {
    let result = create_decoder(Codec::H264, PipelineMode::Auto);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().codec(), Codec::H264);

    let result = create_decoder(Codec::Hevc, PipelineMode::Auto);
    assert!(matches!(result, Err(VideoError::UnsupportedCodec { .. })));
}

#[cfg(all(
    not(target_os = "macos"),
    not(feature = "fallback"),
    not(feature = "ffmpeg-fallback")
))]
#[test]
fn test_create_decoder_auto_unsupported_on_non_macos_without_fallback() {
    let result = create_decoder(Codec::Hevc, PipelineMode::Auto);
    assert!(matches!(result, Err(VideoError::UnsupportedPlatform)));

    let result = create_decoder(Codec::H264, PipelineMode::Auto);
    assert!(matches!(result, Err(VideoError::UnsupportedPlatform)));
}

#[cfg(target_os = "macos")]
#[test]
fn test_create_decoder_auto_returns_vt_decoder_on_macos() {
    let result = create_decoder(Codec::Hevc, PipelineMode::Auto);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().codec(), Codec::Hevc);

    let result = create_decoder(Codec::H264, PipelineMode::Auto);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().codec(), Codec::H264);
}

// ── Software mode ─────────────────────────────────────────────────────────

#[cfg(all(feature = "fallback", not(feature = "ffmpeg-fallback")))]
#[test]
fn test_create_decoder_software_returns_openh264() {
    let result = create_decoder(Codec::H264, PipelineMode::Software);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().codec(), Codec::H264);
}

#[cfg(all(feature = "fallback", not(feature = "ffmpeg-fallback")))]
#[test]
fn test_create_decoder_software_rejects_hevc() {
    let result = create_decoder(Codec::Hevc, PipelineMode::Software);
    assert!(matches!(result, Err(VideoError::UnsupportedCodec { .. })));
}

#[cfg(not(any(feature = "fallback", feature = "ffmpeg-fallback")))]
#[test]
fn test_create_decoder_software_unsupported_without_fallback() {
    let result = create_decoder(Codec::H264, PipelineMode::Software);
    assert!(matches!(result, Err(VideoError::UnsupportedPlatform)));
}

#[cfg(feature = "ffmpeg-fallback")]
#[test]
fn test_create_decoder_software_returns_ffmpeg_h264() {
    let result = create_decoder(Codec::H264, PipelineMode::Software);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().codec(), Codec::H264);
}

#[cfg(feature = "ffmpeg-fallback")]
#[test]
fn test_create_decoder_software_returns_ffmpeg_hevc() {
    let result = create_decoder(Codec::Hevc, PipelineMode::Software);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().codec(), Codec::Hevc);
}

#[cfg(all(not(target_os = "macos"), feature = "ffmpeg-fallback"))]
#[test]
fn test_create_decoder_auto_returns_ffmpeg_on_non_macos() {
    let result = create_decoder(Codec::H264, PipelineMode::Auto);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().codec(), Codec::H264);

    let result = create_decoder(Codec::Hevc, PipelineMode::Auto);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().codec(), Codec::Hevc);
}

// ── VideoDecoder trait contract ────────────────────────────────────────────

#[test]
fn test_video_decoder_decode_returns_frame() {
    let mut dec = NullDecoder {
        codec: Codec::Hevc,
        emit_frame: true,
        fail_on_corrupt: false,
    };
    let packet = EncodedPacket::new(vec![0u8; 64], true, 1000, 16_667);
    let frame = dec.decode(&packet).unwrap().unwrap();
    assert_eq!(frame.width, 1920);
    assert_eq!(frame.height, 1080);
    assert_eq!(frame.timestamp_us, 1000);
}

#[test]
fn test_video_decoder_decode_returns_none_when_buffering() {
    let mut dec = NullDecoder {
        codec: Codec::H264,
        emit_frame: false,
        fail_on_corrupt: false,
    };
    let packet = EncodedPacket::new(vec![0u8; 64], false, 0, 16_667);
    assert!(dec.decode(&packet).unwrap().is_none());
}

#[test]
fn test_video_decoder_decode_rejects_corrupt_empty_packet() {
    let mut dec = NullDecoder {
        codec: Codec::Hevc,
        emit_frame: false,
        fail_on_corrupt: true,
    };
    let corrupt = EncodedPacket::new(vec![], false, 0, 0);
    let err = dec.decode(&corrupt).unwrap_err();
    assert!(matches!(err, VideoError::CorruptPacket { .. }));
}

#[test]
fn test_video_decoder_flush_returns_empty() {
    let mut dec = NullDecoder {
        codec: Codec::H264,
        emit_frame: false,
        fail_on_corrupt: false,
    };
    assert!(dec.flush().unwrap().is_empty());
}

#[test]
fn test_video_decoder_codec_returns_hevc() {
    let dec = NullDecoder {
        codec: Codec::Hevc,
        emit_frame: false,
        fail_on_corrupt: false,
    };
    assert_eq!(dec.codec(), Codec::Hevc);
}

#[test]
fn test_video_decoder_codec_returns_h264() {
    let dec = NullDecoder {
        codec: Codec::H264,
        emit_frame: false,
        fail_on_corrupt: false,
    };
    assert_eq!(dec.codec(), Codec::H264);
}
