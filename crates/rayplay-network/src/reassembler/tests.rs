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

// ── MAX_IN_FLIGHT_FRAMES constant ─────────────────────────────────────────

#[test]
fn test_max_in_flight_frames_is_16() {
    assert_eq!(MAX_IN_FLIGHT_FRAMES, 16);
}

// ── Large keyframe survives while P-frames advance ────────────────────────

#[allow(clippy::cast_possible_truncation)]
#[test]
fn test_large_keyframe_survives_while_pframes_complete() {
    // Simulates the real bug: a large keyframe (10 fragments) is still
    // assembling while small P-frames (1 fragment each) complete and
    // advance the frame_id. With max_pending=16, the keyframe survives.
    let mut r = VideoReassembler::with_default_max();

    // Start receiving a large keyframe (frame 0, 10 fragments)
    for i in 0..5 {
        r.ingest(make_frag(0, i, 10, FLAG_KEYFRAME, vec![i as u8]));
    }
    assert_eq!(r.pending_count(), 1);

    // Meanwhile, 10 small P-frames arrive and complete (frames 1-10)
    for frame_id in 1..=10 {
        let pkt = r.ingest(single_frag(frame_id, vec![0xFF]));
        assert!(pkt.is_some(), "P-frame {frame_id} should complete");
    }

    // The large keyframe (frame 0) should still be in the buffer
    assert!(
        r.pending.contains_key(&0),
        "Large keyframe should survive while P-frames advance"
    );

    // Now finish the keyframe
    for i in 5..10 {
        let result = r.ingest(make_frag(0, i, 10, FLAG_KEYFRAME, vec![i as u8]));
        if i == 9 {
            // Last fragment completes the keyframe
            let pkt = result.unwrap();
            assert!(pkt.is_keyframe);
            assert_eq!(pkt.data, vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
        } else {
            assert!(result.is_none());
        }
    }
}

#[allow(clippy::cast_possible_truncation)]
#[test]
fn test_old_max_would_have_evicted_large_keyframe() {
    // Prove that with the old max_pending=4, the large keyframe
    // would be evicted prematurely when 4+ incomplete frames coexist.
    let mut r = VideoReassembler::new(4);

    // Start a large keyframe (frame 0, 10 fragments), send 5 of them
    for i in 0..5 {
        r.ingest(make_frag(0, i, 10, FLAG_KEYFRAME, vec![i as u8]));
    }

    // 3 more incomplete frames fill the buffer to 4 total
    r.ingest(make_frag(1, 0, 2, 0, vec![]));
    r.ingest(make_frag(2, 0, 2, 0, vec![]));
    r.ingest(make_frag(3, 0, 2, 0, vec![]));
    assert_eq!(r.pending_count(), 4);

    // 5th frame arrives — evicts frame 0 (oldest), the large keyframe
    r.ingest(make_frag(4, 0, 2, 0, vec![]));

    assert!(
        !r.pending.contains_key(&0),
        "With max_pending=4, keyframe should be evicted"
    );
    assert_eq!(r.pending_count(), 4);
}

#[allow(clippy::cast_possible_truncation)]
#[test]
fn test_16_concurrent_incomplete_frames_all_survive() {
    let mut r = VideoReassembler::with_default_max();

    // Insert 16 incomplete frames (each with frag_total=2, only send first frag)
    for frame_id in 0..16 {
        r.ingest(make_frag(frame_id, 0, 2, 0, vec![frame_id as u8]));
    }

    assert_eq!(r.pending_count(), 16);

    // All 16 should be present
    for frame_id in 0..16 {
        assert!(r.pending.contains_key(&frame_id));
    }
}

#[test]
fn test_17th_frame_evicts_oldest_at_default_max() {
    let mut r = VideoReassembler::with_default_max();

    // Fill to capacity (16 incomplete frames)
    for frame_id in 0..16 {
        r.ingest(make_frag(frame_id, 0, 2, 0, vec![]));
    }
    assert_eq!(r.pending_count(), 16);

    // 17th frame should evict frame 0 (oldest)
    r.ingest(make_frag(16, 0, 2, 0, vec![]));
    assert_eq!(r.pending_count(), 16);
    assert!(!r.pending.contains_key(&0));
    assert!(r.pending.contains_key(&16));
}
