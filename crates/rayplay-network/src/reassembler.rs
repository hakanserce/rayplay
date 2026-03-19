//! Reassembles [`VideoFragment`]s back into [`EncodedPacket`]s.

use std::collections::HashMap;

use rayplay_core::packet::EncodedPacket;

use crate::wire::{FLAG_KEYFRAME, VideoFragment};

/// Maximum number of incomplete frames held in memory simultaneously (ADR-003).
pub const MAX_IN_FLIGHT_FRAMES: usize = 4;

/// Reassembles fragmented video packets from QUIC datagrams.
///
/// Tracks incomplete frames by ID and completes them when all fragments arrive.
/// Automatically discards stale frames that exceed `MAX_IN_FLIGHT_FRAMES`.
pub struct FrameReassembler {
    /// In-progress frame assembly state, keyed by frame ID.
    incomplete_frames: HashMap<u32, IncompleteFrame>,
}

/// State for a frame that's being assembled from fragments.
#[derive(Debug)]
struct IncompleteFrame {
    /// Frame metadata (extracted from first fragment).
    frame_id: u32,
    is_keyframe: bool,
    expected_chunks: u16,

    /// Fragment payloads indexed by fragment index.
    chunks: HashMap<u16, Vec<u8>>,
}

impl FrameReassembler {
    /// Creates a new reassembler instance.
    #[must_use]
    pub fn new() -> Self {
        Self {
            incomplete_frames: HashMap::new(),
        }
    }

    /// Adds a fragment and returns a completed packet if this was the final fragment.
    ///
    /// Returns `None` if more fragments are needed, or if the fragment is invalid
    /// (e.g., duplicate fragment index for the same frame).
    pub fn add_fragment(&mut self, fragment: VideoFragment) -> Option<EncodedPacket> {
        // Handle memory pressure by evicting old incomplete frames
        if self.incomplete_frames.len() >= MAX_IN_FLIGHT_FRAMES {
            let oldest_key = *self.incomplete_frames.keys().min()?;
            self.incomplete_frames.remove(&oldest_key);
        }

        let frame = self
            .incomplete_frames
            .entry(fragment.frame_id)
            .or_insert_with(|| IncompleteFrame {
                frame_id: fragment.frame_id,
                is_keyframe: (fragment.flags & FLAG_KEYFRAME) != 0,
                expected_chunks: fragment.frag_total,
                chunks: HashMap::new(),
            });

        // Ignore duplicate fragments
        if frame.chunks.contains_key(&fragment.frag_index) {
            return None;
        }

        // Insert the fragment payload
        frame.chunks.insert(fragment.frag_index, fragment.payload);

        // Check if frame is complete
        if frame.chunks.len() == frame.expected_chunks as usize {
            let completed_frame = self.incomplete_frames.remove(&fragment.frame_id)?;
            Some(Self::reassemble_frame(completed_frame))
        } else {
            None
        }
    }

    /// Combines fragment payloads into a single encoded packet.
    #[allow(clippy::needless_pass_by_value)] // We need to consume the frame to move out its chunks
    fn reassemble_frame(frame: IncompleteFrame) -> EncodedPacket {
        let mut payload = Vec::new();

        // Concatenate chunks in fragment index order
        for seq in 0..frame.expected_chunks {
            if let Some(chunk_data) = frame.chunks.get(&seq) {
                payload.extend_from_slice(chunk_data);
            }
        }

        // Derive timestamp from frame_id for consistency with fragmenter
        let timestamp_us = u64::from(frame.frame_id);
        let duration_us = 16_667; // Default 60fps duration

        EncodedPacket::new(payload, frame.is_keyframe, timestamp_us, duration_us)
    }

    /// Returns the number of incomplete frames currently in memory.
    #[must_use]
    pub fn in_flight_count(&self) -> usize {
        self.incomplete_frames.len()
    }

    /// Clears all incomplete frames (useful for testing).
    pub fn clear(&mut self) {
        self.incomplete_frames.clear();
    }
}

impl Default for FrameReassembler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::{Channel, VideoFragment};

    fn create_test_fragment(
        frame_id: u32,
        frag_index: u16,
        frag_total: u16,
        payload: Vec<u8>,
    ) -> VideoFragment {
        VideoFragment {
            frame_id,
            frag_index,
            frag_total,
            flags: 0,
            channel: Channel::Video,
            payload,
        }
    }

    #[test]
    fn test_reassembler_single_fragment_frame() {
        let mut reassembler = FrameReassembler::new();
        let frag = create_test_fragment(100, 0, 1, vec![1, 2, 3]);

        let packet = reassembler.add_fragment(frag).expect("should complete");
        assert_eq!(packet.data, vec![1, 2, 3]);
        assert_eq!(packet.timestamp_us, 100);
        assert_eq!(reassembler.in_flight_count(), 0);
    }

    #[test]
    fn test_reassembler_multi_fragment_frame() {
        let mut reassembler = FrameReassembler::new();

        // Add fragments out of order
        let frag2 = create_test_fragment(200, 1, 3, vec![4, 5, 6]);
        assert!(reassembler.add_fragment(frag2).is_none());
        assert_eq!(reassembler.in_flight_count(), 1);

        let frag0 = create_test_fragment(200, 0, 3, vec![1, 2, 3]);
        assert!(reassembler.add_fragment(frag0).is_none());

        let frag1 = create_test_fragment(200, 2, 3, vec![7, 8]);
        let packet = reassembler.add_fragment(frag1).expect("should complete");

        assert_eq!(packet.data, vec![1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(reassembler.in_flight_count(), 0);
    }

    #[test]
    fn test_reassembler_ignores_duplicate_fragments() {
        let mut reassembler = FrameReassembler::new();
        let frag = create_test_fragment(300, 0, 2, vec![1, 2]);

        // Add same fragment twice
        assert!(reassembler.add_fragment(frag.clone()).is_none());
        assert!(reassembler.add_fragment(frag).is_none()); // Should be ignored

        assert_eq!(reassembler.in_flight_count(), 1);
        let frame_data = &reassembler.incomplete_frames[&300];
        assert_eq!(frame_data.chunks.len(), 1); // Still only one chunk
    }

    #[test]
    fn test_reassembler_memory_pressure_eviction() {
        let mut reassembler = FrameReassembler::new();

        // Fill up to capacity
        for i in 0..MAX_IN_FLIGHT_FRAMES {
            let frag = create_test_fragment(i as u32, 0, 2, vec![i as u8]);
            reassembler.add_fragment(frag);
        }
        assert_eq!(reassembler.in_flight_count(), MAX_IN_FLIGHT_FRAMES);

        // Adding one more should evict the oldest (frame 0)
        let overflow_frag = create_test_fragment(999, 0, 2, vec![99]);
        reassembler.add_fragment(overflow_frag);

        assert_eq!(reassembler.in_flight_count(), MAX_IN_FLIGHT_FRAMES);
        assert!(!reassembler.incomplete_frames.contains_key(&0)); // Evicted
        assert!(reassembler.incomplete_frames.contains_key(&999)); // Present
    }

    #[test]
    fn test_reassembler_keyframe_flag_preservation() {
        let mut reassembler = FrameReassembler::new();
        let mut keyframe_frag = create_test_fragment(400, 0, 1, vec![0xAB]);
        keyframe_frag.flags = FLAG_KEYFRAME;

        let packet = reassembler
            .add_fragment(keyframe_frag)
            .expect("should complete");
        assert!(packet.is_keyframe);
    }

    #[test]
    fn test_reassembler_clear() {
        let mut reassembler = FrameReassembler::new();
        let frag = create_test_fragment(500, 0, 2, vec![1]);
        reassembler.add_fragment(frag);

        assert_eq!(reassembler.in_flight_count(), 1);
        reassembler.clear();
        assert_eq!(reassembler.in_flight_count(), 0);
    }

    #[test]
    fn test_reassembler_default_trait() {
        let reassembler = FrameReassembler::default();
        assert_eq!(reassembler.in_flight_count(), 0);
    }

    #[test]
    fn test_reassembler_concurrent_frames() {
        let mut reassembler = FrameReassembler::new();

        // Start two different frames
        let frag_a1 = create_test_fragment(1000, 0, 2, vec![1, 2]);
        let frag_b1 = create_test_fragment(2000, 0, 2, vec![10, 20]);
        reassembler.add_fragment(frag_a1);
        reassembler.add_fragment(frag_b1);
        assert_eq!(reassembler.in_flight_count(), 2);

        // Complete frame B first
        let frag_b2 = create_test_fragment(2000, 1, 2, vec![30]);
        let packet_b = reassembler.add_fragment(frag_b2).expect("B complete");
        assert_eq!(packet_b.data, vec![10, 20, 30]);
        assert_eq!(reassembler.in_flight_count(), 1);

        // Complete frame A
        let frag_a2 = create_test_fragment(1000, 1, 2, vec![3]);
        let packet_a = reassembler.add_fragment(frag_a2).expect("A complete");
        assert_eq!(packet_a.data, vec![1, 2, 3]);
        assert_eq!(reassembler.in_flight_count(), 0);
    }
}