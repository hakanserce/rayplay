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
mod tests {
    use super::*;

    #[test]
    fn test_encoded_packet_new_stores_fields() {
        let data = vec![0u8, 1, 2, 3];
        let pkt = EncodedPacket::new(data.clone(), true, 1000, 16_667);
        assert_eq!(pkt.data, data);
        assert!(pkt.is_keyframe);
        assert_eq!(pkt.timestamp_us, 1000);
        assert_eq!(pkt.duration_us, 16_667);
    }

    #[test]
    fn test_encoded_packet_len() {
        let pkt = EncodedPacket::new(vec![0u8; 128], false, 0, 0);
        assert_eq!(pkt.len(), 128);
    }

    #[test]
    fn test_encoded_packet_is_empty_true_for_empty_data() {
        let pkt = EncodedPacket::new(vec![], false, 0, 0);
        assert!(pkt.is_empty());
    }

    #[test]
    fn test_encoded_packet_is_empty_false_for_non_empty_data() {
        let pkt = EncodedPacket::new(vec![0u8], false, 0, 0);
        assert!(!pkt.is_empty());
    }

    #[test]
    fn test_encoded_packet_clone() {
        let pkt = EncodedPacket::new(vec![1, 2, 3], true, 42, 100);
        let cloned = pkt.clone();
        assert_eq!(cloned.data, pkt.data);
        assert_eq!(cloned.is_keyframe, pkt.is_keyframe);
        assert_eq!(cloned.timestamp_us, pkt.timestamp_us);
        assert_eq!(cloned.duration_us, pkt.duration_us);
    }

    #[test]
    fn test_encoded_packet_non_keyframe() {
        let pkt = EncodedPacket::new(vec![0u8; 64], false, 16_667, 16_667);
        assert!(!pkt.is_keyframe);
    }
}
