//! Software H.264 encoder using the `openh264` crate.
//!
//! Provides a cross-platform fallback when hardware encoders (NVENC) are
//! unavailable. Gated behind the `fallback` Cargo feature.

use openh264::encoder::Encoder;
use openh264::formats::YUVBuffer;

use crate::encoder::{Codec, EncoderConfig, EncoderInput, VideoEncoder, VideoError};
use crate::packet::EncodedPacket;

/// Software H.264 encoder backed by `openh264`.
pub struct OpenH264Encoder {
    // Note: openh264::encoder::Encoder does not implement Debug
    encoder: Encoder,
    config: EncoderConfig,
}

impl std::fmt::Debug for OpenH264Encoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenH264Encoder")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl OpenH264Encoder {
    /// Creates a new `OpenH264Encoder` with the given configuration.
    ///
    /// # Errors
    ///
    /// Returns [`VideoError::UnsupportedCodec`] if the codec is not H.264,
    /// or [`VideoError::EncodingFailed`] if the encoder cannot be initialized.
    pub fn new(config: EncoderConfig) -> Result<Self, VideoError> {
        if config.codec != Codec::H264 {
            return Err(VideoError::UnsupportedCodec {
                codec: config.codec.clone(),
            });
        }

        // OpenH264 requires even dimensions for 4:2:0 chroma subsampling.
        if !config.width.is_multiple_of(2) || !config.height.is_multiple_of(2) {
            return Err(VideoError::InvalidDimensions {
                width: config.width,
                height: config.height,
            });
        }

        let api_config = openh264::encoder::EncoderConfig::new()
            .set_bitrate_bps(config.resolved_bitrate())
            .max_frame_rate(f32::from(u16::try_from(config.fps).unwrap_or(u16::MAX)));

        let api = openh264::OpenH264API::from_source();
        let encoder =
            Encoder::with_api_config(api, api_config).map_err(|e| VideoError::EncodingFailed {
                reason: format!("OpenH264 encoder init failed: {e}"),
            })?;

        Ok(Self { encoder, config })
    }
}

/// Converts BGRA pixels to a `YUVBuffer` (BT.601 full-range) in a single pass
/// using fixed-point integer arithmetic.
///
/// Processes pixels in 2x2 blocks: computes Y for each pixel and accumulates
/// U/V sums for chroma subsampling, avoiding a second pass over the data.
///
/// Fixed-point coefficients use a 16-bit shift (multiply by 65 536) so all
/// arithmetic stays in `i32`, which is substantially faster than `f32` on most
/// CPUs and avoids rounding-mode surprises.
#[allow(
    clippy::many_single_char_names,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn bgra_to_yuv(data: &[u8], width: u32, height: u32, stride: u32) -> YUVBuffer {
    // Fixed-point BT.601 coefficients (×65 536)
    const FP_SHIFT: i32 = 16;
    const FP_HALF: i32 = 1 << 15; // 0.5 in fixed-point for rounding

    // Y coefficients
    const Y_R: i32 = 19_595; // 0.299 × 65536
    const Y_G: i32 = 38_470; // 0.587 × 65536
    const Y_B: i32 = 7_471; // 0.114 × 65536

    // U coefficients
    const U_R: i32 = -11_076; // -0.169 × 65536
    const U_G: i32 = -21_692; // -0.331 × 65536
    const U_B: i32 = 32_768; //  0.500 × 65536

    // V coefficients
    const V_R: i32 = 32_768; //  0.500 × 65536
    const V_G: i32 = -27_460; // -0.419 × 65536
    const V_B: i32 = -5_308; // -0.081 × 65536

    let img_w = width as usize;
    let img_h = height as usize;
    let row_stride = stride as usize;
    let uv_w = img_w / 2;
    let uv_h = img_h / 2;

    let yuv_size = img_w * img_h + 2 * uv_w * uv_h;
    let mut yuv = vec![0u8; yuv_size];

    let (y_plane, uv_planes) = yuv.split_at_mut(img_w * img_h);
    let (u_plane, v_plane) = uv_planes.split_at_mut(uv_w * uv_h);

    // Single pass: iterate 2×2 blocks, compute Y per pixel and accumulate U/V.
    for block_row in 0..uv_h {
        for block_col in 0..uv_w {
            let mut sum_u: i32 = 0;
            let mut sum_v: i32 = 0;

            for dy in 0..2 {
                let py = block_row * 2 + dy;
                let row_offset = py * row_stride;
                for dx in 0..2 {
                    let px_col = block_col * 2 + dx;
                    let px = row_offset + px_col * 4;
                    let b = i32::from(data[px]);
                    let g = i32::from(data[px + 1]);
                    let r = i32::from(data[px + 2]);

                    // Y (clamp to 0..255)
                    let y_val = (Y_R * r + Y_G * g + Y_B * b + FP_HALF) >> FP_SHIFT;
                    y_plane[py * img_w + px_col] = y_val.clamp(0, 255) as u8;

                    // Accumulate U/V for the 2×2 block
                    sum_u += U_R * r + U_G * g + U_B * b;
                    sum_v += V_R * r + V_G * g + V_B * b;
                }
            }

            // Average over 4 pixels, add 128 offset, round and clamp.
            let u_val = ((sum_u + 2) / 4 + (128 << FP_SHIFT) + FP_HALF) >> FP_SHIFT;
            let v_val = ((sum_v + 2) / 4 + (128 << FP_SHIFT) + FP_HALF) >> FP_SHIFT;
            u_plane[block_row * uv_w + block_col] = u_val.clamp(0, 255) as u8;
            v_plane[block_row * uv_w + block_col] = v_val.clamp(0, 255) as u8;
        }
    }

    YUVBuffer::from_vec(yuv, img_w, img_h)
}

impl VideoEncoder for OpenH264Encoder {
    fn encode(&mut self, input: EncoderInput<'_>) -> Result<Option<EncodedPacket>, VideoError> {
        let frame = match input {
            EncoderInput::Cpu(f) => f,
            EncoderInput::GpuTexture { .. } => {
                return Err(VideoError::EncodingFailed {
                    reason: "OpenH264Encoder does not support GPU textures".to_string(),
                });
            }
        };

        if frame.width != self.config.width || frame.height != self.config.height {
            return Err(VideoError::InvalidDimensions {
                width: frame.width,
                height: frame.height,
            });
        }

        let yuv = bgra_to_yuv(&frame.data, frame.width, frame.height, frame.stride);

        let bitstream = self
            .encoder
            .encode(&yuv)
            .map_err(|e| VideoError::EncodingFailed {
                reason: format!("OpenH264 encode failed: {e}"),
            })?;

        let data = bitstream.to_vec();
        if data.is_empty() {
            return Ok(None);
        }

        let duration_us = if self.config.fps > 0 {
            1_000_000 / u64::from(self.config.fps)
        } else {
            0
        };

        Ok(Some(EncodedPacket::new(
            data,
            true, // OpenH264 baseline profile: every frame is a keyframe by default
            frame.timestamp_us,
            duration_us,
        )))
    }

    fn flush(&mut self) -> Result<Vec<EncodedPacket>, VideoError> {
        Ok(vec![])
    }

    fn config(&self) -> &EncoderConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
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
}
