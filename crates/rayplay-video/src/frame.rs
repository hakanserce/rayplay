/// A raw, uncompressed video frame as produced by a screen capture API.
///
/// The pixel layout is BGRA 8-bit, matching DXGI Desktop Duplication output.
/// `stride` is the number of bytes per row (may be larger than `width * 4`
/// due to GPU alignment requirements).
#[derive(Debug, Clone)]
pub struct RawFrame {
    /// Raw pixel bytes in BGRA format.
    pub data: Vec<u8>,
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Row stride in bytes (bytes per row, including any padding).
    pub stride: u32,
    /// Capture timestamp in microseconds (monotonic clock).
    pub timestamp_us: u64,
}

impl RawFrame {
    /// Creates a new raw frame.
    #[must_use]
    pub fn new(data: Vec<u8>, width: u32, height: u32, stride: u32, timestamp_us: u64) -> Self {
        Self {
            data,
            width,
            height,
            stride,
            timestamp_us,
        }
    }

    /// Returns the expected byte size of the frame without stride padding.
    ///
    /// Useful for validating that `data` is large enough before passing
    /// to an encoder.
    #[must_use]
    pub fn expected_size(&self) -> usize {
        (self.width as usize) * (self.height as usize) * 4
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raw_frame_new_stores_fields() {
        let data = vec![1u8, 2, 3, 4];
        let frame = RawFrame::new(data.clone(), 1, 1, 4, 1000);
        assert_eq!(frame.data, data);
        assert_eq!(frame.width, 1);
        assert_eq!(frame.height, 1);
        assert_eq!(frame.stride, 4);
        assert_eq!(frame.timestamp_us, 1000);
    }

    #[test]
    fn test_raw_frame_expected_size_1080p() {
        let frame = RawFrame::new(vec![], 1920, 1080, 1920 * 4, 0);
        assert_eq!(frame.expected_size(), 1920 * 1080 * 4);
    }

    #[test]
    fn test_raw_frame_expected_size_4k() {
        let frame = RawFrame::new(vec![], 3840, 2160, 3840 * 4, 0);
        assert_eq!(frame.expected_size(), 3840 * 2160 * 4);
    }

    #[test]
    fn test_raw_frame_expected_size_zero_for_empty_dimensions() {
        let frame = RawFrame::new(vec![], 0, 0, 0, 0);
        assert_eq!(frame.expected_size(), 0);
    }

    #[test]
    fn test_raw_frame_clone() {
        let frame = RawFrame::new(vec![0xABu8; 8], 2, 1, 8, 999);
        let cloned = frame.clone();
        assert_eq!(cloned.data, frame.data);
        assert_eq!(cloned.timestamp_us, frame.timestamp_us);
    }
}
