//! Reassembles [`VideoFragment`]s back into [`EncodedPacket`]s.

use std::collections::HashMap;

use rayplay_video::packet::EncodedPacket;

use crate::wire::{FLAG_KEYFRAME, VideoFragment};

/// Maximum number of incomplete frames held in memory simultaneously (ADR-003).
pub const MAX_IN_FLIGHT_FRAMES: usize = 4;

/// State for a single partially-received frame.
struct PendingFrame {
    frag_total: u16,
    is_keyframe: bool,
    fragments: Vec<Option<Vec<u8>>>,
    received: u16,
}

impl PendingFrame {
    fn new(frag_total: u16, is_keyframe: bool) -> Self {
        Self {
            frag_total,
            is_keyframe,
            fragments: vec![None; usize::from(frag_total)],
            received: 0,
        }
    }

    /// Returns `true` if all fragments have been received.
    fn is_complete(&self) -> bool {
        self.received == self.frag_total
    }

    /// Assembles all fragments into a contiguous payload in order.
    fn assemble(self) -> Vec<u8> {
        self.fragments.into_iter().flatten().flatten().collect()
    }
}

/// Reassembles [`VideoFragment`]s into complete [`EncodedPacket`]s.
///
/// Bounded to at most `max_pending` incomplete frames at a time. When a new
/// `frame_id` arrives and the buffer is full, the oldest incomplete frame is
/// evicted (dropped) to make room — matching the ADR-003 "drop oldest" policy.
pub struct VideoReassembler {
    pending: HashMap<u32, PendingFrame>,
    max_pending: usize,
}

impl VideoReassembler {
    /// Creates a new reassembler with the given maximum number of in-flight frames.
    ///
    /// # Panics
    ///
    /// Panics if `max_pending` is zero.
    #[must_use]
    pub fn new(max_pending: usize) -> Self {
        assert!(max_pending > 0, "max_pending must be > 0");
        Self {
            pending: HashMap::new(),
            max_pending,
        }
    }

    /// Creates a reassembler using [`MAX_IN_FLIGHT_FRAMES`] (4).
    #[must_use]
    pub fn with_default_max() -> Self {
        Self::new(MAX_IN_FLIGHT_FRAMES)
    }

    /// Ingests one fragment.
    ///
    /// Returns `Some(EncodedPacket)` when all fragments for a frame arrive;
    /// `None` otherwise.
    ///
    /// # Drop / eviction semantics
    ///
    /// - When the buffer is full and a fragment for a *new* `frame_id` arrives,
    ///   the frame with the lowest `frame_id` is evicted.
    /// - Duplicate fragments (slot already filled) are silently ignored.
    /// - Fragments whose `frag_index >= existing frag_total` are silently ignored
    ///   (inconsistent sender — protect against out-of-bounds).
    pub fn ingest(&mut self, frag: VideoFragment) -> Option<EncodedPacket> {
        let frame_id = frag.frame_id;

        // If this frame_id isn't yet tracked and we're at capacity, evict oldest.
        if !self.pending.contains_key(&frame_id) {
            if self.pending.len() >= self.max_pending {
                self.evict_oldest();
            }
            let is_keyframe = frag.flags & FLAG_KEYFRAME != 0;
            self.pending
                .insert(frame_id, PendingFrame::new(frag.frag_total, is_keyframe));
        }

        let entry = self.pending.get_mut(&frame_id)?;

        // Guard against inconsistent frag_index vs the stored frag_total.
        let idx = usize::from(frag.frag_index);
        if idx >= entry.fragments.len() {
            return None;
        }

        // Ignore duplicate fragments.
        if entry.fragments[idx].is_some() {
            return None;
        }

        entry.fragments[idx] = Some(frag.payload);
        entry.received += 1;

        if entry.is_complete() {
            let frame = self.pending.remove(&frame_id)?;
            let is_keyframe = frame.is_keyframe;
            let data = frame.assemble();
            Some(EncodedPacket::new(data, is_keyframe, 0, 0))
        } else {
            None
        }
    }

    /// Evicts all incomplete frames whose `frame_id` is strictly less than
    /// `before_frame_id`.
    ///
    /// Returns the number of frames evicted.
    pub fn evict_before(&mut self, before_frame_id: u32) -> usize {
        let keys_to_remove: Vec<u32> = self
            .pending
            .keys()
            .copied()
            .filter(|&k| k < before_frame_id)
            .collect();
        let count = keys_to_remove.len();
        for k in keys_to_remove {
            self.pending.remove(&k);
        }
        count
    }

    /// Returns the number of frames currently buffered (incomplete).
    #[must_use]
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Evicts the frame with the smallest `frame_id` (oldest in-flight frame).
    fn evict_oldest(&mut self) {
        if let Some(&oldest) = self.pending.keys().min() {
            self.pending.remove(&oldest);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::{Channel, FLAG_KEYFRAME, VideoFragment};

    fn make_frag(
        frame_id: u32,
        frag_index: u16,
        frag_total: u16,
        flags: u8,
        payload: Vec<u8>,
    ) -> VideoFragment {
        VideoFragment {
            frame_id,
            frag_index,
            frag_total,
            channel: Channel::Video,
            flags,
            payload,
        }
    }

    fn single_frag(frame_id: u32, data: Vec<u8>) -> VideoFragment {
        make_frag(frame_id, 0, 1, 0, data)
    }

    // ── Constructor ───────────────────────────────────────────────────────────

    #[test]
    fn test_new_stores_max_pending() {
        let r = VideoReassembler::new(8);
        assert_eq!(r.max_pending, 8);
    }

    #[test]
    #[should_panic(expected = "max_pending must be > 0")]
    fn test_new_zero_panics() {
        let _ = VideoReassembler::new(0);
    }

    #[test]
    fn test_with_default_max_uses_constant() {
        let r = VideoReassembler::with_default_max();
        assert_eq!(r.max_pending, MAX_IN_FLIGHT_FRAMES);
    }

    #[test]
    fn test_initial_pending_count_is_zero() {
        let r = VideoReassembler::new(4);
        assert_eq!(r.pending_count(), 0);
    }

    // ── ingest: single-fragment frames ────────────────────────────────────────

    #[test]
    fn test_ingest_single_fragment_returns_packet() {
        let mut r = VideoReassembler::new(4);
        let result = r.ingest(single_frag(0, vec![1, 2, 3]));
        assert!(result.is_some());
        assert_eq!(result.unwrap().data, vec![1, 2, 3]);
    }

    #[test]
    fn test_ingest_single_fragment_clears_pending() {
        let mut r = VideoReassembler::new(4);
        r.ingest(single_frag(0, vec![1]));
        assert_eq!(r.pending_count(), 0);
    }

    #[test]
    fn test_ingest_keyframe_flag_propagated() {
        let mut r = VideoReassembler::new(4);
        let frag = make_frag(0, 0, 1, FLAG_KEYFRAME, vec![0xAA]);
        let pkt = r.ingest(frag).unwrap();
        assert!(pkt.is_keyframe);
    }

    #[test]
    fn test_ingest_non_keyframe_flag_not_set() {
        let mut r = VideoReassembler::new(4);
        let frag = make_frag(0, 0, 1, 0, vec![0xAA]);
        let pkt = r.ingest(frag).unwrap();
        assert!(!pkt.is_keyframe);
    }

    // ── ingest: multi-fragment frames ─────────────────────────────────────────

    #[test]
    fn test_ingest_multi_fragment_returns_none_until_complete() {
        let mut r = VideoReassembler::new(4);
        assert!(r.ingest(make_frag(0, 0, 3, 0, vec![1])).is_none());
        assert!(r.ingest(make_frag(0, 1, 3, 0, vec![2])).is_none());
    }

    #[test]
    fn test_ingest_multi_fragment_returns_packet_on_last() {
        let mut r = VideoReassembler::new(4);
        r.ingest(make_frag(0, 0, 3, 0, vec![1]));
        r.ingest(make_frag(0, 1, 3, 0, vec![2]));
        let pkt = r.ingest(make_frag(0, 2, 3, 0, vec![3])).unwrap();
        assert_eq!(pkt.data, vec![1, 2, 3]);
    }

    #[test]
    fn test_ingest_multi_fragment_out_of_order() {
        let mut r = VideoReassembler::new(4);
        r.ingest(make_frag(0, 2, 3, 0, vec![3]));
        r.ingest(make_frag(0, 0, 3, 0, vec![1]));
        let pkt = r.ingest(make_frag(0, 1, 3, 0, vec![2])).unwrap();
        assert_eq!(pkt.data, vec![1, 2, 3]);
    }

    #[test]
    fn test_ingest_multi_fragment_payload_concatenated_in_order() {
        let mut r = VideoReassembler::new(4);
        r.ingest(make_frag(0, 1, 3, 0, vec![0xBB]));
        r.ingest(make_frag(0, 2, 3, 0, vec![0xCC]));
        let pkt = r.ingest(make_frag(0, 0, 3, 0, vec![0xAA])).unwrap();
        assert_eq!(pkt.data, vec![0xAA, 0xBB, 0xCC]);
    }

    // ── ingest: duplicate fragments ───────────────────────────────────────────

    #[test]
    fn test_ingest_duplicate_fragment_ignored() {
        let mut r = VideoReassembler::new(4);
        r.ingest(make_frag(0, 0, 2, 0, vec![1]));
        // Send frag 0 again — should be ignored
        assert!(r.ingest(make_frag(0, 0, 2, 0, vec![99])).is_none());
        // Frame not yet complete (still need frag 1)
        assert_eq!(r.pending_count(), 1);
    }

    #[test]
    fn test_ingest_duplicate_does_not_corrupt_payload() {
        let mut r = VideoReassembler::new(4);
        r.ingest(make_frag(0, 0, 2, 0, vec![0xAA]));
        r.ingest(make_frag(0, 0, 2, 0, vec![0xFF])); // duplicate, ignored
        let pkt = r.ingest(make_frag(0, 1, 2, 0, vec![0xBB])).unwrap();
        assert_eq!(pkt.data, vec![0xAA, 0xBB]);
    }

    // ── ingest: inconsistent frag_index ───────────────────────────────────────

    #[test]
    fn test_ingest_frag_index_out_of_range_ignored() {
        let mut r = VideoReassembler::new(4);
        // frag_total=2 but frag_index=5 for this fragment
        r.ingest(make_frag(0, 0, 2, 0, vec![1]));
        // Second fragment has inconsistent frag_index >= stored frag_total
        let bad = VideoFragment {
            frame_id: 0,
            frag_index: 5,
            frag_total: 2,
            channel: Channel::Video,
            flags: 0,
            payload: vec![99],
        };
        assert!(r.ingest(bad).is_none());
        assert_eq!(r.pending_count(), 1);
    }

    // ── ingest: eviction when at capacity ─────────────────────────────────────

    #[test]
    fn test_ingest_evicts_oldest_when_at_capacity() {
        let mut r = VideoReassembler::new(2);
        // Fill to capacity with frames 0 and 1 (both incomplete: frag_total=2)
        r.ingest(make_frag(0, 0, 2, 0, vec![1]));
        r.ingest(make_frag(1, 0, 2, 0, vec![2]));
        assert_eq!(r.pending_count(), 2);
        // New frame 2 arrives — frame 0 (oldest) should be evicted
        r.ingest(make_frag(2, 0, 2, 0, vec![3]));
        assert_eq!(r.pending_count(), 2);
        assert!(!r.pending.contains_key(&0));
        assert!(r.pending.contains_key(&1));
        assert!(r.pending.contains_key(&2));
    }

    // ── evict_before ──────────────────────────────────────────────────────────

    #[test]
    fn test_evict_before_removes_older_frames() {
        let mut r = VideoReassembler::new(10);
        r.ingest(make_frag(0, 0, 2, 0, vec![]));
        r.ingest(make_frag(1, 0, 2, 0, vec![]));
        r.ingest(make_frag(5, 0, 2, 0, vec![]));
        let evicted = r.evict_before(5);
        assert_eq!(evicted, 2);
        assert_eq!(r.pending_count(), 1);
        assert!(r.pending.contains_key(&5));
    }

    #[test]
    fn test_evict_before_zero_evicts_nothing() {
        let mut r = VideoReassembler::new(10);
        r.ingest(make_frag(0, 0, 2, 0, vec![]));
        let evicted = r.evict_before(0);
        assert_eq!(evicted, 0);
        assert_eq!(r.pending_count(), 1);
    }

    #[test]
    fn test_evict_before_all_evicts_everything() {
        let mut r = VideoReassembler::new(10);
        r.ingest(make_frag(0, 0, 2, 0, vec![]));
        r.ingest(make_frag(1, 0, 2, 0, vec![]));
        r.ingest(make_frag(2, 0, 2, 0, vec![]));
        let evicted = r.evict_before(100);
        assert_eq!(evicted, 3);
        assert_eq!(r.pending_count(), 0);
    }

    #[test]
    fn test_evict_before_empty_reassembler() {
        let mut r = VideoReassembler::new(4);
        assert_eq!(r.evict_before(10), 0);
    }

    // ── pending_count ─────────────────────────────────────────────────────────

    #[test]
    fn test_pending_count_increases_on_new_frame() {
        let mut r = VideoReassembler::new(4);
        r.ingest(make_frag(0, 0, 2, 0, vec![]));
        assert_eq!(r.pending_count(), 1);
        r.ingest(make_frag(1, 0, 2, 0, vec![]));
        assert_eq!(r.pending_count(), 2);
    }

    #[test]
    fn test_pending_count_decreases_on_complete_frame() {
        let mut r = VideoReassembler::new(4);
        r.ingest(make_frag(0, 0, 2, 0, vec![]));
        assert_eq!(r.pending_count(), 1);
        r.ingest(make_frag(0, 1, 2, 0, vec![])); // completes frame 0
        assert_eq!(r.pending_count(), 0);
    }

    // ── multiple concurrent frames ────────────────────────────────────────────

    #[test]
    fn test_multiple_interleaved_frames_reassemble_correctly() {
        let mut r = VideoReassembler::new(4);
        r.ingest(make_frag(0, 0, 2, 0, vec![0xA0]));
        r.ingest(make_frag(1, 0, 2, 0, vec![0xB0]));
        let p0 = r.ingest(make_frag(0, 1, 2, 0, vec![0xA1])).unwrap();
        let p1 = r.ingest(make_frag(1, 1, 2, 0, vec![0xB1])).unwrap();
        assert_eq!(p0.data, vec![0xA0, 0xA1]);
        assert_eq!(p1.data, vec![0xB0, 0xB1]);
    }
}
