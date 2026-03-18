//! Software video decoder using FFmpeg via the `ffmpeg-next` crate.
//!
//! Supports both H.264 and HEVC decoding, providing broader codec coverage
//! than the OpenH264 fallback (H.264 only).  Gated behind the
//! `ffmpeg-fallback` Cargo feature.

use ffmpeg_next::codec::{self, Id};
use ffmpeg_next::decoder::Video as VideoDec;
use ffmpeg_next::format::Pixel;
use ffmpeg_next::frame;
use ffmpeg_next::software::scaling;

use crate::decoded_frame::{DecodedFrame, PixelFormat};
use crate::decoder::VideoDecoder;
use crate::encoder::{Codec, VideoError};
use crate::packet::EncodedPacket;

/// Software video decoder backed by FFmpeg (H.264 + HEVC).
pub struct FfmpegDecoder {
    decoder: VideoDec,
    scaler: Option<scaling::Context>,
    scaler_params: Option<(u32, u32, Pixel)>,
    codec: Codec,
}

impl std::fmt::Debug for FfmpegDecoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FfmpegDecoder")
            .field("codec", &self.codec)
            .finish_non_exhaustive()
    }
}

// SAFETY: FFmpeg decoder contexts are not thread-safe for concurrent access,
// but are safe to move between threads (same single-owner model as OpenH264).
unsafe impl Send for FfmpegDecoder {}

impl FfmpegDecoder {
    /// Creates a new `FfmpegDecoder` for the specified codec.
    ///
    /// # Errors
    ///
    /// Returns [`VideoError::DecodingFailed`] if the FFmpeg decoder cannot be
    /// initialized.
    pub fn new(codec: Codec) -> Result<Self, VideoError> {
        ffmpeg_next::init().map_err(|e| VideoError::DecodingFailed {
            reason: format!("FFmpeg init failed: {e}"),
        })?;

        let codec_id = match codec {
            Codec::H264 => Id::H264,
            Codec::Hevc => Id::HEVC,
        };

        let ff_codec =
            codec::decoder::find(codec_id).ok_or_else(|| VideoError::DecodingFailed {
                reason: format!("FFmpeg decoder not found for {codec_id:?}"),
            })?;

        let ctx = codec::context::Context::new_with_codec(ff_codec);
        let decoder = ctx
            .decoder()
            .video()
            .map_err(|e| VideoError::DecodingFailed {
                reason: format!("FFmpeg decoder context creation failed: {e}"),
            })?;

        Ok(Self {
            decoder,
            scaler: None,
            scaler_params: None,
            codec,
        })
    }

    fn ensure_scaler(
        &mut self,
        width: u32,
        height: u32,
        src_format: Pixel,
    ) -> Result<&mut scaling::Context, VideoError> {
        let current_params = (width, height, src_format);
        if self.scaler_params.as_ref() != Some(&current_params) {
            self.scaler = None;
        }
        if self.scaler.is_none() {
            let scaler = scaling::Context::get(
                src_format,
                width,
                height,
                Pixel::BGRA,
                width,
                height,
                scaling::Flags::BILINEAR,
            )
            .map_err(|e| VideoError::DecodingFailed {
                reason: format!("FFmpeg scaler creation failed: {e}"),
            })?;
            self.scaler = Some(scaler);
            self.scaler_params = Some(current_params);
        }
        Ok(self.scaler.as_mut().expect("scaler just set"))
    }

    fn yuv_to_decoded_frame(
        &mut self,
        decoded: &frame::Video,
        timestamp_us: u64,
    ) -> Result<DecodedFrame, VideoError> {
        let width = decoded.width();
        let height = decoded.height();
        let src_format = decoded.format();

        let scaler = self.ensure_scaler(width, height, src_format)?;

        let mut bgra_frame = frame::Video::new(Pixel::BGRA, width, height);
        scaler
            .run(decoded, &mut bgra_frame)
            .map_err(|e| VideoError::DecodingFailed {
                reason: format!("FFmpeg YUV→BGRA scaling failed: {e}"),
            })?;

        #[allow(clippy::cast_possible_truncation)]
        let stride = bgra_frame.stride(0) as u32;
        #[allow(clippy::cast_possible_truncation)]
        let data_len = (stride * height) as usize;
        let bgra_data = bgra_frame.data(0)[..data_len].to_vec();

        Ok(DecodedFrame::new_cpu(
            bgra_data,
            width,
            height,
            stride,
            PixelFormat::Bgra8,
            timestamp_us,
        ))
    }
}

impl VideoDecoder for FfmpegDecoder {
    fn decode(&mut self, packet: &EncodedPacket) -> Result<Option<DecodedFrame>, VideoError> {
        let pkt = ffmpeg_next::Packet::copy(&packet.data);
        self.decoder
            .send_packet(&pkt)
            .map_err(|e| VideoError::DecodingFailed {
                reason: format!("FFmpeg send_packet failed: {e}"),
            })?;

        let mut decoded = frame::Video::empty();
        match self.decoder.receive_frame(&mut decoded) {
            Ok(()) => {}
            Err(ffmpeg_next::Error::Other {
                errno: libc::EAGAIN,
            }) => return Ok(None),
            Err(e) => {
                return Err(VideoError::DecodingFailed {
                    reason: format!("FFmpeg receive_frame failed: {e}"),
                });
            }
        }

        self.yuv_to_decoded_frame(&decoded, packet.timestamp_us)
            .map(Some)
    }

    fn flush(&mut self) -> Result<Vec<DecodedFrame>, VideoError> {
        self.decoder
            .send_eof()
            .map_err(|e| VideoError::DecodingFailed {
                reason: format!("FFmpeg send_eof failed: {e}"),
            })?;

        let mut frames = Vec::new();
        loop {
            let mut decoded = frame::Video::empty();
            match self.decoder.receive_frame(&mut decoded) {
                Ok(()) => {
                    frames.push(self.yuv_to_decoded_frame(&decoded, 0)?);
                }
                Err(_) => break,
            }
        }

        Ok(frames)
    }

    fn codec(&self) -> Codec {
        self.codec.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ffmpeg_decoder_new_h264() {
        let dec = FfmpegDecoder::new(Codec::H264);
        assert!(dec.is_ok());
    }

    #[test]
    fn test_ffmpeg_decoder_new_hevc() {
        let dec = FfmpegDecoder::new(Codec::Hevc);
        assert!(dec.is_ok());
    }

    #[test]
    fn test_ffmpeg_decoder_codec_accessor_h264() {
        let dec = FfmpegDecoder::new(Codec::H264).unwrap();
        assert_eq!(dec.codec(), Codec::H264);
    }

    #[test]
    fn test_ffmpeg_decoder_codec_accessor_hevc() {
        let dec = FfmpegDecoder::new(Codec::Hevc).unwrap();
        assert_eq!(dec.codec(), Codec::Hevc);
    }

    #[test]
    fn test_ffmpeg_decoder_flush_empty() {
        let mut dec = FfmpegDecoder::new(Codec::H264).unwrap();
        let frames = dec.flush().unwrap();
        assert!(frames.is_empty());
    }

    #[test]
    fn test_ffmpeg_decoder_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<FfmpegDecoder>();
    }

    #[test]
    fn test_ffmpeg_decoder_debug_impl() {
        let dec = FfmpegDecoder::new(Codec::H264).unwrap();
        let dbg = format!("{dec:?}");
        assert!(dbg.contains("FfmpegDecoder"));
        assert!(dbg.contains("H264"));
    }

    #[test]
    fn test_ffmpeg_round_trip_h264() {
        use crate::encoder::{EncoderConfig, EncoderInput, VideoEncoder};
        use crate::ffmpeg_enc::FfmpegEncoder;
        use crate::frame::RawFrame;

        let config = EncoderConfig::with_codec(64, 64, 30, Codec::H264);
        let mut encoder = FfmpegEncoder::new(config).unwrap();

        let mut data = vec![0u8; 64 * 64 * 4];
        for pixel in data.chunks_exact_mut(4) {
            pixel[0] = 100; // B
            pixel[1] = 150; // G
            pixel[2] = 200; // R
            pixel[3] = 255; // A
        }
        let frame = RawFrame::new(data, 64, 64, 64 * 4, 42_000);

        // Encode multiple frames to ensure we get output
        let mut packets = Vec::new();
        for _ in 0..5 {
            if let Some(pkt) = encoder.encode(EncoderInput::Cpu(&frame)).unwrap() {
                packets.push(pkt);
            }
        }
        packets.extend(encoder.flush().unwrap());
        assert!(
            !packets.is_empty(),
            "encoder should produce at least one packet"
        );

        let mut decoder = FfmpegDecoder::new(Codec::H264).unwrap();
        let mut decoded_any = false;
        for pkt in &packets {
            if let Some(decoded_frame) = decoder.decode(pkt).unwrap() {
                assert_eq!(decoded_frame.width, 64);
                assert_eq!(decoded_frame.height, 64);
                assert_eq!(decoded_frame.format, PixelFormat::Bgra8);
                assert!(!decoded_frame.data.is_empty());
                decoded_any = true;
            }
        }
        // Also flush decoder
        let flushed = decoder.flush().unwrap();
        decoded_any = decoded_any || !flushed.is_empty();
        assert!(decoded_any, "decoder should produce at least one frame");
    }

    #[test]
    fn test_ffmpeg_round_trip_hevc() {
        use crate::encoder::{EncoderConfig, EncoderInput, VideoEncoder};
        use crate::ffmpeg_enc::FfmpegEncoder;
        use crate::frame::RawFrame;

        let config = EncoderConfig::with_codec(64, 64, 30, Codec::Hevc);
        let mut encoder = FfmpegEncoder::new(config).unwrap();

        let frame = RawFrame::new(vec![128u8; 64 * 64 * 4], 64, 64, 64 * 4, 0);

        let mut packets = Vec::new();
        for _ in 0..5 {
            if let Some(pkt) = encoder.encode(EncoderInput::Cpu(&frame)).unwrap() {
                packets.push(pkt);
            }
        }
        packets.extend(encoder.flush().unwrap());
        assert!(
            !packets.is_empty(),
            "HEVC encoder should produce at least one packet"
        );

        let mut decoder = FfmpegDecoder::new(Codec::Hevc).unwrap();
        let mut decoded_any = false;
        for pkt in &packets {
            if let Some(decoded_frame) = decoder.decode(pkt).unwrap() {
                assert_eq!(decoded_frame.width, 64);
                assert_eq!(decoded_frame.height, 64);
                decoded_any = true;
            }
        }
        let flushed = decoder.flush().unwrap();
        decoded_any = decoded_any || !flushed.is_empty();
        assert!(
            decoded_any,
            "HEVC decoder should produce at least one frame"
        );
    }
}
