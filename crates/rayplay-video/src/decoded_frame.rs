/// Pixel format of a decoded video frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PixelFormat {
    /// BGRA 8-bit interleaved — CPU-side representation, GPU-uploadable directly.
    Bgra8,
    /// YUV 4:2:0 biplanar (NV12) — native `VideoToolbox` output format on macOS.
    ///
    /// Plane 0 is luma (Y), plane 1 is interleaved chroma (UV).
    Nv12,
}

impl PixelFormat {
    /// Returns the bytes per pixel for packed formats, or `None` for planar formats.
    ///
    /// `Nv12` returns `None` because its byte count depends on frame dimensions;
    /// use `DecodedFrame::expected_data_size` instead.
    #[must_use]
    pub fn bytes_per_pixel(&self) -> Option<usize> {
        match self {
            Self::Bgra8 => Some(4),
            Self::Nv12 => None,
        }
    }
}

/// A decoded video frame ready for rendering.
///
/// On macOS, hardware-decoded frames are backed by a `CVPixelBuffer` (`IOSurface`).
/// When `is_hardware_frame` is `true`, `data` is empty and the frame lives in
/// GPU-resident memory; the renderer imports it via `IOSurface` interop (ADR-005)
/// rather than uploading via `write_texture`.
#[derive(Debug, Clone)]
pub struct DecodedFrame {
    /// Decoded pixel data. Empty when `is_hardware_frame` is `true`.
    pub data: Vec<u8>,
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Row stride in bytes (first-plane stride for NV12; interleaved stride for BGRA).
    pub stride: u32,
    /// Pixel format of the decoded data.
    pub format: PixelFormat,
    /// Presentation timestamp in microseconds.
    pub timestamp_us: u64,
    /// Whether the frame is backed by GPU-resident `IOSurface` memory (macOS).
    pub is_hardware_frame: bool,
}

impl DecodedFrame {
    /// Creates a CPU-side decoded frame (software decode or mock).
    #[must_use]
    pub fn new_cpu(
        data: Vec<u8>,
        width: u32,
        height: u32,
        stride: u32,
        format: PixelFormat,
        timestamp_us: u64,
    ) -> Self {
        Self {
            data,
            width,
            height,
            stride,
            format,
            timestamp_us,
            is_hardware_frame: false,
        }
    }

    /// Creates a hardware-backed frame placeholder (`IOSurface` on macOS).
    ///
    /// `data` is left empty; the frame data lives in GPU memory.
    #[must_use]
    pub fn new_hardware(
        width: u32,
        height: u32,
        stride: u32,
        format: PixelFormat,
        timestamp_us: u64,
    ) -> Self {
        Self {
            data: Vec::new(),
            width,
            height,
            stride,
            format,
            timestamp_us,
            is_hardware_frame: true,
        }
    }

    /// Expected byte size of `data` for CPU frames.
    ///
    /// - `Bgra8`: `stride × height`
    /// - `Nv12`: `stride × height × 3 / 2` (luma plane + half-height chroma plane)
    /// - Hardware frames: `0` (data is GPU-resident)
    #[must_use]
    pub fn expected_data_size(&self) -> usize {
        if self.is_hardware_frame {
            return 0;
        }
        let h = self.height as usize;
        let s = self.stride as usize;
        match self.format {
            PixelFormat::Bgra8 => s * h,
            PixelFormat::Nv12 => s * h * 3 / 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── PixelFormat ────────────────────────────────────────────────────────────

    #[test]
    fn test_pixel_format_bgra8_bytes_per_pixel() {
        assert_eq!(PixelFormat::Bgra8.bytes_per_pixel(), Some(4));
    }

    #[test]
    fn test_pixel_format_nv12_bytes_per_pixel_is_none() {
        assert_eq!(PixelFormat::Nv12.bytes_per_pixel(), None);
    }

    #[test]
    fn test_pixel_format_clone_and_eq() {
        let a = PixelFormat::Bgra8;
        let b = a.clone();
        assert_eq!(a, b);
        assert_ne!(a, PixelFormat::Nv12);
    }

    #[test]
    fn test_pixel_format_debug() {
        assert!(format!("{:?}", PixelFormat::Bgra8).contains("Bgra8"));
        assert!(format!("{:?}", PixelFormat::Nv12).contains("Nv12"));
    }

    // ── DecodedFrame::new_cpu ──────────────────────────────────────────────────

    #[test]
    fn test_decoded_frame_new_cpu_stores_all_fields() {
        let data = vec![1u8, 2, 3, 4];
        let frame = DecodedFrame::new_cpu(data.clone(), 1, 1, 4, PixelFormat::Bgra8, 1000);
        assert_eq!(frame.data, data);
        assert_eq!(frame.width, 1);
        assert_eq!(frame.height, 1);
        assert_eq!(frame.stride, 4);
        assert_eq!(frame.format, PixelFormat::Bgra8);
        assert_eq!(frame.timestamp_us, 1000);
        assert!(!frame.is_hardware_frame);
    }

    #[test]
    fn test_decoded_frame_new_cpu_clone() {
        let frame = DecodedFrame::new_cpu(vec![0xABu8; 8], 2, 1, 8, PixelFormat::Nv12, 42);
        let cloned = frame.clone();
        assert_eq!(cloned.data, frame.data);
        assert_eq!(cloned.timestamp_us, frame.timestamp_us);
        assert_eq!(cloned.format, frame.format);
    }

    // ── DecodedFrame::new_hardware ─────────────────────────────────────────────

    #[test]
    fn test_decoded_frame_new_hardware_has_empty_data() {
        let frame = DecodedFrame::new_hardware(1920, 1080, 1920 * 4, PixelFormat::Nv12, 0);
        assert!(frame.data.is_empty());
        assert!(frame.is_hardware_frame);
        assert_eq!(frame.width, 1920);
        assert_eq!(frame.height, 1080);
        assert_eq!(frame.format, PixelFormat::Nv12);
    }

    // ── expected_data_size ─────────────────────────────────────────────────────

    #[test]
    fn test_expected_data_size_hardware_frame_is_zero() {
        let frame = DecodedFrame::new_hardware(1920, 1080, 1920 * 4, PixelFormat::Nv12, 0);
        assert_eq!(frame.expected_data_size(), 0);
    }

    #[test]
    fn test_expected_data_size_bgra8_1080p() {
        let frame = DecodedFrame::new_cpu(vec![], 1920, 1080, 1920 * 4, PixelFormat::Bgra8, 0);
        assert_eq!(frame.expected_data_size(), 1920 * 4 * 1080);
    }

    #[test]
    fn test_expected_data_size_bgra8_4k() {
        let frame = DecodedFrame::new_cpu(vec![], 3840, 2160, 3840 * 4, PixelFormat::Bgra8, 0);
        assert_eq!(frame.expected_data_size(), 3840 * 4 * 2160);
    }

    #[test]
    fn test_expected_data_size_nv12_1080p() {
        // NV12: stride * height * 3 / 2 = 1920 * 1080 * 3 / 2
        let frame = DecodedFrame::new_cpu(vec![], 1920, 1080, 1920, PixelFormat::Nv12, 0);
        assert_eq!(frame.expected_data_size(), 1920 * 1080 * 3 / 2);
    }

    #[test]
    fn test_expected_data_size_zero_dimensions() {
        let frame = DecodedFrame::new_cpu(vec![], 0, 0, 0, PixelFormat::Bgra8, 0);
        assert_eq!(frame.expected_data_size(), 0);
    }
}
