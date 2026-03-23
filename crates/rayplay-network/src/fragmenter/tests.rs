use super::*;
use rayplay_core::packet::EncodedPacket;

fn make_packet(size: usize, is_keyframe: bool) -> EncodedPacket {
    EncodedPacket::new(vec![0xABu8; size], is_keyframe, 0, 16_667)
}

// ── Constructor ───────────────────────────────────────────────────────────

#[test]
fn test_new_stores_max_payload() {
    let f = VideoFragmenter::new(500);
    assert_eq!(f.max_payload, 500);
}

#[test]
#[should_panic(expected = "max_payload must be > 0")]
fn test_new_zero_payload_panics() {
    let _ = VideoFragmenter::new(0);
}

#[test]
fn test_with_default_payload_uses_max_fragment_payload() {
    let f = VideoFragmenter::with_default_payload();
    assert_eq!(f.max_payload, MAX_FRAGMENT_PAYLOAD);
}

#[test]
fn test_initial_frame_counter_is_zero() {
    let f = VideoFragmenter::new(100);
    assert_eq!(f.frame_counter(), 0);
}

// ── fragment: empty packet ────────────────────────────────────────────────

#[test]
fn test_fragment_empty_packet_returns_empty_vec() {
    let mut f = VideoFragmenter::new(100);
    let pkt = make_packet(0, false);
    let frags = f.fragment(&pkt);
    assert!(frags.is_empty());
}

#[test]
fn test_fragment_empty_packet_does_not_increment_frame_counter() {
    let mut f = VideoFragmenter::new(100);
    let pkt = make_packet(0, false);
    let _ = f.fragment(&pkt);
    assert_eq!(f.frame_counter(), 0);
}

// ── fragment: single fragment ─────────────────────────────────────────────

#[test]
fn test_fragment_single_chunk_produces_one_fragment() {
    let mut f = VideoFragmenter::new(100);
    let pkt = make_packet(50, false);
    let frags = f.fragment(&pkt);
    assert_eq!(frags.len(), 1);
}

#[test]
fn test_fragment_single_chunk_frag_total_is_one() {
    let mut f = VideoFragmenter::new(100);
    let frags = f.fragment(&make_packet(50, false));
    assert_eq!(frags[0].frag_total, 1);
}

#[test]
fn test_fragment_single_chunk_frag_index_is_zero() {
    let mut f = VideoFragmenter::new(100);
    let frags = f.fragment(&make_packet(50, false));
    assert_eq!(frags[0].frag_index, 0);
}

// ── fragment: multiple fragments ──────────────────────────────────────────

#[test]
fn test_fragment_exact_boundary_produces_correct_count() {
    let mut f = VideoFragmenter::new(100);
    // 200 bytes / 100 bytes per chunk = 2 fragments
    let frags = f.fragment(&make_packet(200, false));
    assert_eq!(frags.len(), 2);
}

#[test]
fn test_fragment_over_boundary_produces_extra_fragment() {
    let mut f = VideoFragmenter::new(100);
    // 201 bytes → 3 fragments (100 + 100 + 1)
    let frags = f.fragment(&make_packet(201, false));
    assert_eq!(frags.len(), 3);
    assert_eq!(frags[2].payload.len(), 1);
}

#[test]
fn test_fragment_indices_are_sequential() {
    let mut f = VideoFragmenter::new(100);
    let frags = f.fragment(&make_packet(250, false));
    for (i, frag) in frags.iter().enumerate() {
        assert_eq!(usize::from(frag.frag_index), i);
    }
}

#[test]
fn test_fragment_all_share_same_frame_id() {
    let mut f = VideoFragmenter::new(100);
    let frags = f.fragment(&make_packet(300, false));
    let frame_id = frags[0].frame_id;
    for frag in &frags {
        assert_eq!(frag.frame_id, frame_id);
    }
}

// ── fragment: keyframe flag ───────────────────────────────────────────────

#[test]
fn test_fragment_keyframe_sets_flag_on_all_fragments() {
    let mut f = VideoFragmenter::new(100);
    let frags = f.fragment(&make_packet(250, true));
    for frag in &frags {
        assert!(frag.is_keyframe(), "expected FLAG_KEYFRAME on all frags");
    }
}

#[test]
fn test_fragment_non_keyframe_has_no_flag() {
    let mut f = VideoFragmenter::new(100);
    let frags = f.fragment(&make_packet(50, false));
    assert!(!frags[0].is_keyframe());
}

// ── fragment: frame_id monotonic & wrapping ───────────────────────────────

#[test]
fn test_frame_counter_increments_per_non_empty_packet() {
    let mut f = VideoFragmenter::new(100);
    let _ = f.fragment(&make_packet(10, false));
    assert_eq!(f.frame_counter(), 1);
    let _ = f.fragment(&make_packet(10, false));
    assert_eq!(f.frame_counter(), 2);
}

#[test]
fn test_frame_id_wraps_at_u32_max() {
    let mut f = VideoFragmenter::new(100);
    f.frame_counter = u32::MAX;
    let frags = f.fragment(&make_packet(10, false));
    assert_eq!(frags[0].frame_id, u32::MAX);
    assert_eq!(f.frame_counter(), 0); // wrapped
}

// ── fragment: channel ─────────────────────────────────────────────────────

#[test]
fn test_fragment_channel_is_video() {
    let mut f = VideoFragmenter::new(100);
    let frags = f.fragment(&make_packet(10, false));
    assert_eq!(frags[0].channel, Channel::Video);
}

// ── fragment: payload content ─────────────────────────────────────────────

#[test]
fn test_fragment_payload_reassembles_to_original() {
    let data: Vec<u8> = (0u8..=255).collect();
    let pkt = EncodedPacket::new(data.clone(), false, 0, 0);
    let mut f = VideoFragmenter::new(64);
    let frags = f.fragment(&pkt);

    let reassembled: Vec<u8> = frags.into_iter().flat_map(|fr| fr.payload).collect();
    assert_eq!(reassembled, data);
}

#[test]
fn test_fragment_single_byte_packet() {
    let mut f = VideoFragmenter::new(100);
    let frags = f.fragment(&EncodedPacket::new(vec![0x42], false, 0, 0));
    assert_eq!(frags.len(), 1);
    assert_eq!(frags[0].payload, vec![0x42]);
}

#[test]
#[should_panic(expected = "too many fragments")]
fn test_fragment_panics_when_fragment_count_exceeds_u16_max() {
    // max_payload=1 means each byte becomes its own fragment;
    // 65_536 bytes → 65_536 fragments which overflows u16.
    let mut f = VideoFragmenter::new(1);
    let huge = EncodedPacket::new(vec![0u8; 65_536], false, 0, 0);
    let _ = f.fragment(&huge);
}
