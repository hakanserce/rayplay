//! Software video encoder using FFmpeg via the `ffmpeg-next` crate.
//!
//! Supports both H.264 (`libx264`) and HEVC (`libx265`), providing broader
//! codec coverage than the OpenH264 fallback.  Gated behind the
//! `ffmpeg-fallback` Cargo feature.

use ffmpeg_next::codec::{self, Id};
use ffmpeg_next::encoder::Video as VideoEnc;
use ffmpeg_next::format::Pixel;
use ffmpeg_next::software::scaling;
use ffmpeg_next::{Dictionary, Rational, frame};

use crate::encoder::{Codec, EncoderConfig, EncoderInput, VideoEncoder, VideoError};
use crate::packet::EncodedPacket;

/// Software video encoder backed by FFmpeg (`libx264` / `libx265`).
pub struct FfmpegEncoder {
    encoder: VideoEnc,
    scaler: scaling::Context,
    config: EncoderConfig,
    frame_index: i64,
}

impl std::fmt::Debug for FfmpegEncoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FfmpegEncoder")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

// SAFETY: The FFmpeg encoder context (`AVCodecContext`) is a self-contained,
// heap-allocated structure with no thread-local state. It is safe to move
// between threads as long as only one thread accesses it at a time, which
// `&mut self` on every method guarantees.
unsafe impl Send for FfmpegEncoder {}

impl FfmpegEncoder {
    /// Creates a new `FfmpegEncoder` with the given configuration.
    ///
    /// # Errors
    ///
    /// Returns [`VideoError::InvalidDimensions`] if width or height is odd,
    /// or [`VideoError::EncodingFailed`] if the FFmpeg encoder cannot be opened.
    pub fn new(config: EncoderConfig) -> Result<Self, VideoError> {
        if config.width % 2 != 0 || config.height % 2 != 0 {
            return Err(VideoError::InvalidDimensions {
                width: config.width,
                height: config.height,
            });
        }

        // Idempotent — safe to call from both encoder and decoder constructors
        ffmpeg_next::init().map_err(|e| VideoError::EncodingFailed {
            reason: format!("FFmpeg init failed: {e}"),
        })?;

        let codec_id = match config.codec {
            Codec::H264 => Id::H264,
            Codec::Hevc => Id::HEVC,
        };

        let codec = codec::encoder::find(codec_id).ok_or_else(|| VideoError::EncodingFailed {
            reason: format!("FFmpeg encoder not found for {codec_id:?}"),
        })?;

        let mut ctx = codec::context::Context::new_with_codec(codec)
            .encoder()
            .video()
            .map_err(|e| VideoError::EncodingFailed {
                reason: format!("FFmpeg encoder context creation failed: {e}"),
            })?;

        ctx.set_width(config.width);
        ctx.set_height(config.height);
        ctx.set_format(Pixel::YUV420P);
        ctx.set_time_base(Rational::new(
            1,
            i32::from(u16::try_from(config.fps).unwrap_or(u16::MAX)),
        ));
        ctx.set_bit_rate(config.resolved_bitrate().into());

        let mut opts = Dictionary::new();
        // ultrafast/zerolatency are valid for both libx264 and libx265
        opts.set("preset", "ultrafast");
        opts.set("tune", "zerolatency");

        let encoder = ctx
            .open_with(opts)
            .map_err(|e| VideoError::EncodingFailed {
                reason: format!("FFmpeg encoder open failed: {e}"),
            })?;

        // Eager scaler — dimensions are known at construction time
        let scaler = scaling::Context::get(
            Pixel::BGRA,
            config.width,
            config.height,
            Pixel::YUV420P,
            config.width,
            config.height,
            scaling::Flags::BILINEAR,
        )
        .map_err(|e| VideoError::EncodingFailed {
            reason: format!("FFmpeg scaler creation failed: {e}"),
        })?;

        Ok(Self {
            encoder,
            scaler,
            config,
            frame_index: 0,
        })
    }

    fn duration_us(&self) -> u64 {
        if self.config.fps > 0 {
            1_000_000 / u64::from(self.config.fps)
        } else {
            0
        }
    }
}

impl VideoEncoder for FfmpegEncoder {
    fn encode(&mut self, input: EncoderInput<'_>) -> Result<Option<EncodedPacket>, VideoError> {
        let raw = match input {
            EncoderInput::Cpu(f) => f,
            EncoderInput::GpuTexture { .. } => {
                return Err(VideoError::EncodingFailed {
                    reason: "FfmpegEncoder does not support GPU textures".to_string(),
                });
            }
        };

        if raw.width != self.config.width || raw.height != self.config.height {
            return Err(VideoError::InvalidDimensions {
                width: raw.width,
                height: raw.height,
            });
        }

        // Build BGRA input frame
        let mut bgra_frame = frame::Video::new(Pixel::BGRA, self.config.width, self.config.height);
        let dst_stride = bgra_frame.stride(0);
        let dst_data = bgra_frame.data_mut(0);
        for row in 0..self.config.height as usize {
            let src_start = row * raw.stride as usize;
            let src_end = src_start + self.config.width as usize * 4;
            let dst_start = row * dst_stride;
            let dst_end = dst_start + self.config.width as usize * 4;
            dst_data[dst_start..dst_end].copy_from_slice(&raw.data[src_start..src_end]);
        }

        // Scale BGRA → YUV420P
        let mut yuv_frame =
            frame::Video::new(Pixel::YUV420P, self.config.width, self.config.height);
        self.scaler
            .run(&bgra_frame, &mut yuv_frame)
            .map_err(|e| VideoError::EncodingFailed {
                reason: format!("FFmpeg scaling failed: {e}"),
            })?;

        yuv_frame.set_pts(Some(self.frame_index));
        self.frame_index += 1;

        // Send frame to encoder
        self.encoder
            .send_frame(&yuv_frame)
            .map_err(|e| VideoError::EncodingFailed {
                reason: format!("FFmpeg send_frame failed: {e}"),
            })?;

        // Try to receive encoded packet
        let mut pkt = ffmpeg_next::Packet::empty();
        match self.encoder.receive_packet(&mut pkt) {
            Ok(()) => Ok(Some(EncodedPacket::new(
                pkt.data().unwrap_or(&[]).to_vec(),
                pkt.is_key(),
                raw.timestamp_us,
                self.duration_us(),
            ))),
            Err(ffmpeg_next::Error::Other {
                errno: libc::EAGAIN,
            }) => Ok(None),
            Err(e) => Err(VideoError::EncodingFailed {
                reason: format!("FFmpeg receive_packet failed: {e}"),
            }),
        }
    }

    fn flush(&mut self) -> Result<Vec<EncodedPacket>, VideoError> {
        self.encoder
            .send_eof()
            .map_err(|e| VideoError::EncodingFailed {
                reason: format!("FFmpeg send_eof failed: {e}"),
            })?;

        let mut packets = Vec::new();
        let duration_us = self.duration_us();

        loop {
            let mut pkt = ffmpeg_next::Packet::empty();
            match self.encoder.receive_packet(&mut pkt) {
                Ok(()) => {
                    let ts = pkt.pts().unwrap_or(0);
                    packets.push(EncodedPacket::new(
                        pkt.data().unwrap_or(&[]).to_vec(),
                        pkt.is_key(),
                        u64::try_from(ts).unwrap_or(0),
                        duration_us,
                    ));
                }
                Err(_) => break,
            }
        }

        Ok(packets)
    }

    fn config(&self) -> &EncoderConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoder::{Bitrate, Codec, GpuTextureHandle};
    use crate::frame::RawFrame;

    fn make_config(width: u32, height: u32, fps: u32, codec: Codec) -> EncoderConfig {
        EncoderConfig::with_codec(width, height, fps, codec)
    }

    #[test]
    fn test_ffmpeg_encoder_new_h264() {
        let enc = FfmpegEncoder::new(make_config(64, 64, 30, Codec::H264));
        assert!(enc.is_ok());
    }

    #[test]
    fn test_ffmpeg_encoder_new_hevc() {
        let enc = FfmpegEncoder::new(make_config(64, 64, 30, Codec::Hevc));
        assert!(enc.is_ok());
    }

    #[test]
    fn test_ffmpeg_encoder_rejects_odd_width() {
        let err = FfmpegEncoder::new(make_config(63, 64, 30, Codec::H264)).unwrap_err();
        assert!(matches!(
            err,
            VideoError::InvalidDimensions {
                width: 63,
                height: 64
            }
        ));
    }

    #[test]
    fn test_ffmpeg_encoder_rejects_odd_height() {
        let err = FfmpegEncoder::new(make_config(64, 63, 30, Codec::H264)).unwrap_err();
        assert!(matches!(
            err,
            VideoError::InvalidDimensions {
                width: 64,
                height: 63
            }
        ));
    }

    #[test]
    fn test_ffmpeg_encoder_rejects_gpu_texture() {
        let mut enc = FfmpegEncoder::new(make_config(64, 64, 30, Codec::H264)).unwrap();
        let input = EncoderInput::GpuTexture {
            handle: GpuTextureHandle(std::ptr::null_mut()),
            width: 64,
            height: 64,
            timestamp_us: 0,
        };
        let err = enc.encode(input).unwrap_err();
        assert!(matches!(err, VideoError::EncodingFailed { .. }));
    }

    #[test]
    fn test_ffmpeg_encoder_rejects_wrong_dimensions() {
        let mut enc = FfmpegEncoder::new(make_config(64, 64, 30, Codec::H264)).unwrap();
        let frame = RawFrame::new(vec![0u8; 128 * 128 * 4], 128, 128, 128 * 4, 0);
        let err = enc.encode(EncoderInput::Cpu(&frame)).unwrap_err();
        assert!(matches!(err, VideoError::InvalidDimensions { .. }));
    }

    #[test]
    fn test_ffmpeg_encoder_encodes_h264_frame() {
        let mut enc = FfmpegEncoder::new(make_config(64, 64, 30, Codec::H264)).unwrap();
        let frame = RawFrame::new(vec![128u8; 64 * 64 * 4], 64, 64, 64 * 4, 1000);
        let result = enc.encode(EncoderInput::Cpu(&frame)).unwrap();
        // First frame may or may not produce output depending on encoder buffering
        if let Some(packet) = result {
            assert!(!packet.data.is_empty());
        }
    }

    #[test]
    fn test_ffmpeg_encoder_encodes_hevc_frame() {
        let mut enc = FfmpegEncoder::new(make_config(64, 64, 30, Codec::Hevc)).unwrap();
        let frame = RawFrame::new(vec![128u8; 64 * 64 * 4], 64, 64, 64 * 4, 2000);
        let result = enc.encode(EncoderInput::Cpu(&frame)).unwrap();
        if let Some(packet) = result {
            assert!(!packet.data.is_empty());
        }
    }

    #[test]
    fn test_ffmpeg_encoder_flush() {
        let mut enc = FfmpegEncoder::new(make_config(64, 64, 30, Codec::H264)).unwrap();
        let frame = RawFrame::new(vec![128u8; 64 * 64 * 4], 64, 64, 64 * 4, 0);
        let _ = enc.encode(EncoderInput::Cpu(&frame));
        let flushed = enc.flush().unwrap();
        // Flush may return remaining buffered packets
        for pkt in &flushed {
            assert!(!pkt.data.is_empty());
        }
    }

    #[test]
    fn test_ffmpeg_encoder_config_accessor() {
        let config = make_config(320, 240, 60, Codec::H264).with_bitrate(Bitrate::Mbps(5));
        let enc = FfmpegEncoder::new(config).unwrap();
        assert_eq!(enc.config().width, 320);
        assert_eq!(enc.config().height, 240);
        assert_eq!(enc.config().fps, 60);
        assert_eq!(enc.config().codec, Codec::H264);
        assert_eq!(enc.config().bitrate, Bitrate::Mbps(5));
    }

    #[test]
    fn test_ffmpeg_encoder_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<FfmpegEncoder>();
    }

    #[test]
    fn test_ffmpeg_encoder_debug_impl() {
        let enc = FfmpegEncoder::new(make_config(64, 64, 30, Codec::H264)).unwrap();
        let dbg = format!("{enc:?}");
        assert!(dbg.contains("FfmpegEncoder"));
        assert!(dbg.contains("config"));
    }

    #[test]
    fn test_ffmpeg_encoder_zero_fps_duration() {
        let config = EncoderConfig::with_codec(64, 64, 0, Codec::H264);
        let mut enc = FfmpegEncoder::new(config).expect("encoder should open with 0 fps");
        let frame = RawFrame::new(vec![128u8; 64 * 64 * 4], 64, 64, 64 * 4, 500);
        if let Ok(Some(packet)) = enc.encode(EncoderInput::Cpu(&frame)) {
            assert_eq!(packet.duration_us, 0);
        }
    }

    #[test]
    fn test_ffmpeg_encoder_duration_us() {
        let enc = FfmpegEncoder::new(make_config(64, 64, 30, Codec::H264)).unwrap();
        assert_eq!(enc.duration_us(), 1_000_000 / 30);
    }

    #[test]
    fn test_ffmpeg_encoder_duration_us_zero_fps() {
        let config = EncoderConfig::with_codec(64, 64, 0, Codec::H264);
        let enc = FfmpegEncoder::new(config).expect("encoder should open with 0 fps");
        assert_eq!(enc.duration_us(), 0);
    }
}
