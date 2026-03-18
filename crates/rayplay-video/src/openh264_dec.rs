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
                codec: codec.clone(),
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
        self.codec.clone()
    }
}

#[cfg(test)]
mod tests {
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
        let decoded = decoder.decode(&packet).unwrap();

        // OpenH264 may need multiple frames to produce output; if the first
        // frame is returned, validate it.
        if let Some(decoded_frame) = decoded {
            assert_eq!(decoded_frame.width, 64);
            assert_eq!(decoded_frame.height, 64);
            assert_eq!(decoded_frame.format, PixelFormat::Bgra8);
            assert_eq!(decoded_frame.timestamp_us, 42_000);
            assert!(!decoded_frame.data.is_empty());
            // Verify pixel data is non-zero
            assert!(
                decoded_frame.data.iter().any(|&b| b != 0),
                "decoded frame should have non-zero pixel data"
            );
        }
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
    fn test_openh264_decoder_decode_invalid_data_returns_error_or_none() {
        let mut dec = OpenH264Decoder::new(Codec::H264).unwrap();
        // Feed garbage data — decoder may error or return None
        let packet = EncodedPacket::new(vec![0xFF; 64], false, 0, 16_667);
        let result = dec.decode(&packet);
        // Either Ok(None) or Err — both are acceptable for invalid data
        match result {
            Ok(None) => {} // decoder buffered or ignored
            Ok(Some(_)) => panic!("should not decode garbage into a frame"),
            Err(_) => {} // decoder correctly rejected
        }
    }
}
