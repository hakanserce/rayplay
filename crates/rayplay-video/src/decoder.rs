use crate::{
    decoded_frame::DecodedFrame,
    encoder::{Codec, VideoError},
    packet::EncodedPacket,
};

/// Returns the platform-appropriate hardware decoder.
///
/// On macOS, returns a [`VtDecoder`](crate::videotoolbox::VtDecoder) backed
/// by `VideoToolbox`.  On other platforms returns [`VideoError::UnsupportedPlatform`].
///
/// # Errors
///
/// Returns [`VideoError::UnsupportedPlatform`] on non-macOS, or
/// [`VideoError::DecodingFailed`] if the `VideoToolbox` session cannot be created.
pub fn create_decoder(codec: Codec) -> Result<Box<dyn VideoDecoder>, VideoError> {
    #[cfg(target_os = "macos")]
    {
        use crate::videotoolbox::VtDecoder;
        VtDecoder::new(codec).map(|d| Box::new(d) as Box<dyn VideoDecoder>)
    }
    #[cfg(not(target_os = "macos"))]
    {
        #[cfg(feature = "fallback")]
        {
            use crate::openh264_dec::OpenH264Decoder;
            OpenH264Decoder::new(codec).map(|d| Box::new(d) as Box<dyn VideoDecoder>)
        }
        #[cfg(not(feature = "fallback"))]
        {
            let _ = codec;
            Err(VideoError::UnsupportedPlatform)
        }
    }
}

/// Trait for hardware or software video decoders.
///
/// Implementations must be `Send` so they can be driven from a dedicated
/// decode thread. The `decode` → `flush` lifecycle mirrors the `VideoToolbox`
/// asynchronous session model.
pub trait VideoDecoder: Send {
    /// Submits a compressed packet for decoding.
    ///
    /// Returns `Ok(Some(frame))` when a decoded frame is immediately available,
    /// `Ok(None)` when the decoder is buffering, or an error.
    ///
    /// # Errors
    ///
    /// - `VideoError::CorruptPacket` — bitstream is undecodable (truncated NAL
    ///   units, invalid header, etc.).
    /// - `VideoError::DecodingFailed` — hardware or session error.
    fn decode(&mut self, packet: &EncodedPacket) -> Result<Option<DecodedFrame>, VideoError>;

    /// Flushes any buffered frames and returns all remaining decoded frames.
    ///
    /// Call this at end-of-stream or before reconfiguring the decoder.
    ///
    /// # Errors
    ///
    /// Returns `VideoError::DecodingFailed` if flushing the decode session fails.
    fn flush(&mut self) -> Result<Vec<DecodedFrame>, VideoError>;

    /// Returns the codec this decoder handles.
    fn codec(&self) -> Codec;
}

#[cfg(test)]
mod tests {
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
            self.codec.clone()
        }
    }

    // ── create_decoder factory ─────────────────────────────────────────────────

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn test_create_decoder_unsupported_on_non_macos() {
        let result = create_decoder(Codec::Hevc);
        assert!(matches!(result, Err(VideoError::UnsupportedPlatform)));

        let result = create_decoder(Codec::H264);
        assert!(matches!(result, Err(VideoError::UnsupportedPlatform)));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_create_decoder_returns_vt_decoder_on_macos() {
        let result = create_decoder(Codec::Hevc);
        assert!(result.is_ok());
        let decoder = result.unwrap();
        assert_eq!(decoder.codec(), Codec::Hevc);

        let result = create_decoder(Codec::H264);
        assert!(result.is_ok());
        let decoder = result.unwrap();
        assert_eq!(decoder.codec(), Codec::H264);
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
}
