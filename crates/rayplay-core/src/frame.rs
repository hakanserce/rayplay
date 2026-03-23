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
mod tests;
