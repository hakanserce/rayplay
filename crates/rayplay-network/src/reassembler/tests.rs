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
