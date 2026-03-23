/// A hardware-encoded video packet produced by a `VideoEncoder`.
///
/// Each packet corresponds to one encoded frame (or a slice of a frame).
/// The `data` field contains the raw bitstream bytes suitable for network
/// transmission after chunking by `FrameChunker`.
#[derive(Debug, Clone)]
pub struct EncodedPacket {
    /// Raw encoded bitstream bytes (HEVC NAL units).
    pub data: Vec<u8>,
    /// Whether this packet begins an IDR (keyframe) access unit.
    pub is_keyframe: bool,
    /// Presentation timestamp of the source frame in microseconds.
    pub timestamp_us: u64,
    /// Frame duration in microseconds (e.g., `16_667` µs at 60 fps).
    pub duration_us: u64,
}

impl EncodedPacket {
    /// Creates a new encoded packet.
    #[must_use]
    pub fn new(data: Vec<u8>, is_keyframe: bool, timestamp_us: u64, duration_us: u64) -> Self {
        Self {
            data,
            is_keyframe,
            timestamp_us,
            duration_us,
        }
    }

    /// Returns the size of the encoded bitstream in bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Returns `true` if the bitstream is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

#[cfg(test)]
mod tests;
