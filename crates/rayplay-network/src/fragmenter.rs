//! Splits [`EncodedPacket`]s into [`VideoFragment`]s for QUIC datagram transport.

use rayplay_core::packet::EncodedPacket;

use crate::wire::{Channel, FLAG_KEYFRAME, MAX_FRAGMENT_PAYLOAD, VideoFragment};

/// Splits large video packets into small fragments for QUIC datagram transport.
pub struct FrameFragmenter {
    // No state needed — fragments can be generated on demand.
}

impl FrameFragmenter {
    /// Creates a new fragmenter instance.
    #[must_use]
    pub fn new() -> Self {
        Self {}
    }

    /// Splits a video packet into transmittable fragments.
    ///
    /// Returns an iterator that yields fragments on demand. Each fragment
    /// is sized to fit within a QUIC datagram (< 1280 bytes after headers).
    ///
    /// ## Fragment layout
    ///
    /// ```text
    /// [VideoFragment header: 12 bytes] [payload: 0..MAX_FRAGMENT_PAYLOAD]
    /// ```
    ///
    /// The frame ID is derived from the packet timestamp to ensure reassembly
    /// correctness.
    pub fn fragment(&self, packet: &EncodedPacket) -> impl Iterator<Item = VideoFragment> {
        #[allow(clippy::cast_possible_truncation)]
        let frame_id = packet.timestamp_us as u32; // Use timestamp as unique ID

        // Handle empty packets by ensuring at least one fragment
        let total_chunks = std::cmp::max(1, packet.data.len().div_ceil(MAX_FRAGMENT_PAYLOAD));

        let flags = if packet.is_keyframe {
            FLAG_KEYFRAME
        } else {
            0
        };
        let data = packet.data.clone(); // Clone to avoid lifetime issues

        (0..total_chunks).map(move |chunk_idx| {
            let start_offset = chunk_idx * MAX_FRAGMENT_PAYLOAD;
            let end_offset = std::cmp::min(start_offset + MAX_FRAGMENT_PAYLOAD, data.len());
            let payload = data[start_offset..end_offset].to_vec();

            VideoFragment {
                frame_id,
                #[allow(clippy::cast_possible_truncation)]
                frag_index: chunk_idx as u16,
                #[allow(clippy::cast_possible_truncation)]
                frag_total: total_chunks as u16,
                flags,
                channel: Channel::Video,
                payload,
            }
        })
    }
}

impl Default for FrameFragmenter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rayplay_core::packet::EncodedPacket;

    #[test]
    fn test_fragmenter_single_chunk() {
        let fragmenter = FrameFragmenter::new();
        let packet = EncodedPacket::new(vec![1, 2, 3], true, 1000, 16_667);
        let fragments: Vec<_> = fragmenter.fragment(&packet).collect();

        assert_eq!(fragments.len(), 1);
        assert_eq!(fragments[0].frame_id, 1000);
        assert_eq!(fragments[0].frag_index, 0);
        assert_eq!(fragments[0].frag_total, 1);
        assert_eq!(fragments[0].payload, vec![1, 2, 3]);
        assert_eq!(fragments[0].flags, FLAG_KEYFRAME);
    }

    #[test]
    fn test_fragmenter_multiple_chunks() {
        let fragmenter = FrameFragmenter::new();
        let large_data = vec![0u8; MAX_FRAGMENT_PAYLOAD * 2 + 100]; // Requires 3 chunks
        let packet = EncodedPacket::new(large_data.clone(), false, 2000, 16_667);
        let fragments: Vec<_> = fragmenter.fragment(&packet).collect();

        assert_eq!(fragments.len(), 3);

        // First chunk
        assert_eq!(fragments[0].frag_index, 0);
        assert_eq!(fragments[0].frag_total, 3);
        assert_eq!(fragments[0].payload.len(), MAX_FRAGMENT_PAYLOAD);

        // Second chunk
        assert_eq!(fragments[1].frag_index, 1);
        assert_eq!(fragments[1].frag_total, 3);
        assert_eq!(fragments[1].payload.len(), MAX_FRAGMENT_PAYLOAD);

        // Third chunk (partial)
        assert_eq!(fragments[2].frag_index, 2);
        assert_eq!(fragments[2].frag_total, 3);
        assert_eq!(fragments[2].payload.len(), 100);

        // Verify all fragments have the same frame_id and non-keyframe
        for frag in &fragments {
            assert_eq!(frag.frame_id, 2000);
            assert_eq!(frag.flags, 0); // Not a keyframe
        }
    }

    #[test]
    fn test_fragmenter_empty_packet() {
        let fragmenter = FrameFragmenter::new();
        let packet = EncodedPacket::new(vec![], false, 3000, 16_667);
        let fragments: Vec<_> = fragmenter.fragment(&packet).collect();

        assert_eq!(fragments.len(), 1); // Still produces one fragment
        assert!(fragments[0].payload.is_empty());
    }

    #[test]
    fn test_fragmenter_exact_chunk_boundary() {
        let fragmenter = FrameFragmenter::new();
        let exact_data = vec![0xFFu8; MAX_FRAGMENT_PAYLOAD];
        let packet = EncodedPacket::new(exact_data.clone(), true, 4000, 16_667);
        let fragments: Vec<_> = fragmenter.fragment(&packet).collect();

        assert_eq!(fragments.len(), 1);
        assert_eq!(fragments[0].payload.len(), MAX_FRAGMENT_PAYLOAD);
        assert_eq!(fragments[0].frag_total, 1);
    }

    #[test]
    fn test_fragmenter_keyframe_flag_propagation() {
        let fragmenter = FrameFragmenter::new();

        // Test keyframe
        let keyframe = EncodedPacket::new(vec![0u8; 10], true, 5000, 16_667);
        let key_frags: Vec<_> = fragmenter.fragment(&keyframe).collect();
        assert_eq!(key_frags[0].flags, FLAG_KEYFRAME);

        // Test non-keyframe
        let normal_frame = EncodedPacket::new(vec![0u8; 10], false, 6000, 16_667);
        let normal_frags: Vec<_> = fragmenter.fragment(&normal_frame).collect();
        assert_eq!(normal_frags[0].flags, 0);
    }

    #[test]
    fn test_fragmenter_default_trait() {
        let fragmenter = FrameFragmenter::default();
        let packet = EncodedPacket::new(vec![1], false, 0, 0);
        let fragments: Vec<_> = fragmenter.fragment(&packet).collect();
        assert_eq!(fragments.len(), 1);
    }
}