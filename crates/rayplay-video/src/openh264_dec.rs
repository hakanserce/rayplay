//! Software H.264 decoder using the `openh264` crate.
//!
//! Provides a cross-platform fallback when hardware decoders (`VideoToolbox`)
//! are unavailable. Gated behind the `fallback` Cargo feature.

use openh264::decoder::{Decoder, DecoderConfig};
use openh264::formats::YUVSource;

use crate::decoded_frame::{DecodedFrame, PixelFormat};
use crate::decoder::VideoDecoder;
use crate::encoder::{Codec, VideoError};
use crate::packet::EncodedPacket;

/// Software H.264 decoder backed by `openh264`.
pub struct OpenH264Decoder {
    // Note: openh264::decoder::Decoder does not implement Debug
    decoder: Decoder,
    codec: Codec,
}

impl std::fmt::Debug for OpenH264Decoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenH264Decoder")
            .field("codec", &self.codec)
            .finish_non_exhaustive()
    }
}

impl OpenH264Decoder {
    /// Creates a new `OpenH264Decoder` for the specified codec.
    ///
    /// # Errors
    ///
    /// Returns [`VideoError::UnsupportedCodec`] if the codec is not H.264,
    /// or [`VideoError::DecodingFailed`] if the decoder cannot be initialized.
    pub fn new(codec: Codec) -> Result<Self, VideoError> {
        if codec != Codec::H264 {
            return Err(VideoError::UnsupportedCodec {
                codec: codec.to_string(),
            });
        }

        let api = openh264::OpenH264API::from_source();
        let decoder = Decoder::with_api_config(api, DecoderConfig::new()).map_err(|e| {
            VideoError::DecodingFailed {
                reason: format!("OpenH264 decoder init failed: {e}"),
            }
        })?;

        Ok(Self { decoder, codec })
    }
}

/// Converts a `DecodedYUV` (via `YUVSource` trait) to BGRA8 using fixed-point
/// BT.601 inverse coefficients.
///
/// Fixed-point with a 16-bit shift avoids per-pixel floating-point work.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn yuv_to_bgra(yuv: &impl YUVSource) -> Vec<u8> {
    // Fixed-point BT.601 inverse coefficients (×65 536)
    const FP_SHIFT: i32 = 16;
    const FP_HALF: i32 = 1 << 15;
    const R_V: i32 = 91_881; //  1.402 × 65536
    const G_U: i32 = -22_544; // -0.344 × 65536
    const G_V: i32 = -46_793; // -0.714 × 65536
    const B_U: i32 = 116_130; //  1.772 × 65536

    let (width, height) = yuv.dimensions();
    let (y_stride, u_stride, v_stride) = yuv.strides();
    let y_data = yuv.y();
    let u_data = yuv.u();
    let v_data = yuv.v();

    let mut bgra = vec![0u8; width * height * 4];

    for row in 0..height {
        for col in 0..width {
            let y_val = i32::from(y_data[row * y_stride + col]) << FP_SHIFT;
            let u_val = i32::from(u_data[(row / 2) * u_stride + col / 2]) - 128;
            let v_val = i32::from(v_data[(row / 2) * v_stride + col / 2]) - 128;

            let r = (y_val + R_V * v_val + FP_HALF) >> FP_SHIFT;
            let g = (y_val + G_U * u_val + G_V * v_val + FP_HALF) >> FP_SHIFT;
            let b = (y_val + B_U * u_val + FP_HALF) >> FP_SHIFT;

            let out = (row * width + col) * 4;
            bgra[out] = b.clamp(0, 255) as u8;
            bgra[out + 1] = g.clamp(0, 255) as u8;
            bgra[out + 2] = r.clamp(0, 255) as u8;
            bgra[out + 3] = 255;
        }
    }

    bgra
}

impl VideoDecoder for OpenH264Decoder {
    #[allow(clippy::cast_possible_truncation)]
    fn decode(&mut self, packet: &EncodedPacket) -> Result<Option<DecodedFrame>, VideoError> {
        let maybe_yuv =
            self.decoder
                .decode(&packet.data)
                .map_err(|e| VideoError::DecodingFailed {
                    reason: format!("OpenH264 decode failed: {e}"),
                })?;

        let Some(yuv) = maybe_yuv else {
            return Ok(None);
        };

        let (width, height) = yuv.dimensions();
        let bgra = yuv_to_bgra(&yuv);

        let stride = (width as u32) * 4;
        Ok(Some(DecodedFrame::new_cpu(
            bgra,
            width as u32,
            height as u32,
            stride,
            PixelFormat::Bgra8,
            packet.timestamp_us,
        )))
    }

    fn flush(&mut self) -> Result<Vec<DecodedFrame>, VideoError> {
        Ok(vec![])
    }

    fn codec(&self) -> Codec {
        self.codec
    }
}

#[cfg(test)]
mod tests;
