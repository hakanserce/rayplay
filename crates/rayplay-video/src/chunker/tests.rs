use super::*;

fn make_packet(size: usize, is_keyframe: bool, ts: u64) -> EncodedPacket {
    EncodedPacket::new(vec![0xABu8; size], is_keyframe, ts, 16_667)
}

// ── constructor ────────────────────────────────────────────────────────────

#[test]
fn test_chunker_new_stores_chunk_size() {
    let chunker = FrameChunker::new(512);
    assert_eq!(chunker.max_chunk_size, 512);
}

#[test]
fn test_chunker_with_default_chunk_size() {
    let chunker = FrameChunker::with_default_chunk_size();
    assert_eq!(chunker.max_chunk_size, DEFAULT_CHUNK_SIZE);
}

#[test]
#[should_panic(expected = "max_chunk_size must be greater than zero")]
fn test_chunker_new_panics_on_zero_size() {
    let _ = FrameChunker::new(0);
}

// ── empty packet ───────────────────────────────────────────────────────────

#[test]
fn test_chunk_empty_packet_returns_empty_vec() {
    let mut chunker = FrameChunker::new(1200);
    let packet = make_packet(0, false, 0);
    let chunks = chunker.chunk(&packet);
    assert!(chunks.is_empty());
}

#[test]
fn test_chunk_empty_packet_does_not_increment_counter() {
    let mut chunker = FrameChunker::new(1200);
    chunker.chunk(&make_packet(0, false, 0));
    assert_eq!(chunker.packet_counter(), 0);
}

// ── single chunk (packet fits in one chunk) ────────────────────────────────

#[test]
fn test_chunk_small_packet_produces_one_chunk() {
    let mut chunker = FrameChunker::new(1200);
    let packet = make_packet(64, true, 1000);
    let chunks = chunker.chunk(&packet);
    assert_eq!(chunks.len(), 1);
}

#[test]
fn test_chunk_single_chunk_fields() {
    let mut chunker = FrameChunker::new(1200);
    let packet = make_packet(100, true, 5000);
    let chunks = chunker.chunk(&packet);
    let c = &chunks[0];
    assert_eq!(c.packet_index, 0);
    assert_eq!(c.chunk_index, 0);
    assert_eq!(c.total_chunks, 1);
    assert!(c.is_keyframe);
    assert_eq!(c.timestamp_us, 5000);
    assert_eq!(c.data.len(), 100);
}

// ── multi-chunk splitting ──────────────────────────────────────────────────

#[test]
fn test_chunk_splits_packet_into_correct_count() {
    let mut chunker = FrameChunker::new(1200);
    // 3600 bytes / 1200 per chunk = exactly 3 chunks
    let packet = make_packet(3600, false, 0);
    let chunks = chunker.chunk(&packet);
    assert_eq!(chunks.len(), 3);
}

#[test]
fn test_chunk_last_chunk_smaller_when_uneven() {
    let mut chunker = FrameChunker::new(1200);
    // 2500 bytes → chunks of 1200, 1200, 100
    let packet = make_packet(2500, false, 0);
    let chunks = chunker.chunk(&packet);
    assert_eq!(chunks.len(), 3);
    assert_eq!(chunks[0].data.len(), 1200);
    assert_eq!(chunks[1].data.len(), 1200);
    assert_eq!(chunks[2].data.len(), 100);
}

#[test]
fn test_chunk_total_chunks_matches_split_count() {
    let mut chunker = FrameChunker::new(1200);
    let packet = make_packet(2500, false, 0);
    let chunks = chunker.chunk(&packet);
    for c in &chunks {
        assert_eq!(c.total_chunks, 3);
    }
}

#[test]
fn test_chunk_indices_are_sequential() {
    let mut chunker = FrameChunker::new(500);
    let packet = make_packet(1200, false, 0);
    let chunks = chunker.chunk(&packet);
    for (i, c) in chunks.iter().enumerate() {
        assert_eq!(c.chunk_index, i as u16);
    }
}

#[test]
fn test_chunk_all_chunks_carry_same_packet_index() {
    let mut chunker = FrameChunker::new(500);
    let packet = make_packet(1200, false, 0);
    let chunks = chunker.chunk(&packet);
    for c in &chunks {
        assert_eq!(c.packet_index, 0);
    }
}

// ── packet counter ─────────────────────────────────────────────────────────

#[test]
fn test_packet_counter_increments_per_non_empty_packet() {
    let mut chunker = FrameChunker::new(1200);
    chunker.chunk(&make_packet(100, false, 0));
    chunker.chunk(&make_packet(100, false, 0));
    assert_eq!(chunker.packet_counter(), 2);
}

#[test]
fn test_packet_index_increases_across_calls() {
    let mut chunker = FrameChunker::new(1200);
    let c0 = chunker.chunk(&make_packet(100, false, 0));
    let c1 = chunker.chunk(&make_packet(100, false, 1000));
    assert_eq!(c0[0].packet_index, 0);
    assert_eq!(c1[0].packet_index, 1);
}

#[test]
fn test_packet_counter_wraps_on_overflow() {
    let mut chunker = FrameChunker {
        max_chunk_size: 1200,
        packet_counter: u32::MAX,
    };
    chunker.chunk(&make_packet(100, false, 0));
    assert_eq!(chunker.packet_counter(), 0);
}

// ── data integrity ─────────────────────────────────────────────────────────

#[test]
fn test_chunk_data_reassembly_matches_original() {
    let mut chunker = FrameChunker::new(500);
    let original: Vec<u8> = (0u8..=255u8).cycle().take(1300).collect();
    let packet = EncodedPacket::new(original.clone(), false, 0, 16_667);
    let chunks = chunker.chunk(&packet);
    let reassembled: Vec<u8> = chunks.into_iter().flat_map(|c| c.data).collect();
    assert_eq!(reassembled, original);
}

// ── is_keyframe propagation ────────────────────────────────────────────────

#[test]
fn test_chunk_propagates_is_keyframe_true() {
    let mut chunker = FrameChunker::new(500);
    let packet = make_packet(1000, true, 0);
    for c in chunker.chunk(&packet) {
        assert!(c.is_keyframe);
    }
}

#[test]
fn test_chunk_propagates_is_keyframe_false() {
    let mut chunker = FrameChunker::new(500);
    let packet = make_packet(1000, false, 0);
    for c in chunker.chunk(&packet) {
        assert!(!c.is_keyframe);
    }
}

// ── exact-size boundary ────────────────────────────────────────────────────

#[test]
fn test_chunk_packet_exactly_one_chunk_size() {
    let mut chunker = FrameChunker::new(1200);
    let packet = make_packet(1200, false, 0);
    let chunks = chunker.chunk(&packet);
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].data.len(), 1200);
}

#[test]
fn test_chunk_packet_one_byte_over_chunk_size() {
    let mut chunker = FrameChunker::new(1200);
    let packet = make_packet(1201, false, 0);
    let chunks = chunker.chunk(&packet);
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].data.len(), 1200);
    assert_eq!(chunks[1].data.len(), 1);
}
