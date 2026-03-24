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
#[allow(clippy::cast_possible_truncation)]
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

#[test]
fn test_chunk_advances_packet_counter_once_per_packet() {
    let mut chunker = FrameChunker::new(500);
    let packet1 = make_packet(1200, false, 0);
    let chunks1 = chunker.chunk(&packet1);
    assert!(chunks1.len() > 1);
    for c in &chunks1 {
        assert_eq!(c.packet_index, 0);
    }

    let packet2 = make_packet(800, true, 16667);
    let chunks2 = chunker.chunk(&packet2);
    for c in &chunks2 {
        assert_eq!(c.packet_index, 1);
    }
}

#[test]
fn test_chunk_respects_max_chunk_size() {
    let mut chunker = FrameChunker::new(100);
    let packet = make_packet(250, false, 0);
    let chunks = chunker.chunk(&packet);
    for c in &chunks {
        assert!(c.data.len() <= 100);
    }
}

// ── realism: many packets ──────────────────────────────────────────────────

#[test]
fn test_chunk_realism_streaming_scenario() {
    let mut chunker = FrameChunker::new(1200);

    // Keyframe (large)
    let keyframe_packet = make_packet(8000, true, 0);
    let kf_chunks = chunker.chunk(&keyframe_packet);
    assert!(kf_chunks.len() > 1);
    for c in &kf_chunks {
        assert!(c.is_keyframe);
        assert_eq!(c.packet_index, 0);
    }

    // P-frames (small)
    for frame_idx in 1..=10 {
        let delta_packet = make_packet(600, false, frame_idx * 16_667);
        let delta_chunks = chunker.chunk(&delta_packet);
        assert_eq!(delta_chunks.len(), 1);
        let c = &delta_chunks[0];
        assert!(!c.is_keyframe);
        assert_eq!(u64::from(c.packet_index), frame_idx);
    }
}

// ── edge cases ─────────────────────────────────────────────────────────────

#[test]
fn test_chunk_packet_exactly_max_size() {
    let mut chunker = FrameChunker::new(1200);
    let packet = make_packet(1200, false, 0);
    let chunks = chunker.chunk(&packet);
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].data.len(), 1200);
}

#[test]
fn test_chunk_packet_one_byte_over_max_size() {
    let mut chunker = FrameChunker::new(1200);
    let packet = make_packet(1201, false, 0);
    let chunks = chunker.chunk(&packet);
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].data.len(), 1200);
    assert_eq!(chunks[1].data.len(), 1);
}

#[test]
fn test_chunk_carries_timestamp() {
    let mut chunker = FrameChunker::new(1200);
    let packet = make_packet(100, false, 42_000);
    let chunks = chunker.chunk(&packet);
    assert_eq!(chunks[0].timestamp_us, 42_000);
}

// ── NetworkChunk Debug ────────────────────────────────────────────────────

#[test]
fn test_network_chunk_debug() {
    let chunk = NetworkChunk {
        packet_index: 1,
        chunk_index: 2,
        total_chunks: 5,
        is_keyframe: true,
        timestamp_us: 123_456,
        data: vec![0xAA, 0xBB],
    };
    let dbg = format!("{chunk:?}");
    assert!(dbg.contains('1'));
    assert!(dbg.contains('2'));
    assert!(dbg.contains('5'));
    assert!(dbg.contains("123456"));
}
