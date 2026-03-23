use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rayplay_core::packet::EncodedPacket;
use rayplay_network::{
    FLAG_KEYFRAME, MAX_FRAGMENT_PAYLOAD, VideoFragment, VideoFragmenter, VideoReassembler,
    wire::Channel,
};

fn make_encoded_packet(size: usize, is_keyframe: bool) -> EncodedPacket {
    EncodedPacket::new(vec![0xABu8; size], is_keyframe, 0, 16_667)
}

fn make_fragment(
    frame_id: u32,
    frag_index: u16,
    frag_total: u16,
    payload: Vec<u8>,
) -> VideoFragment {
    VideoFragment {
        frame_id,
        frag_index,
        frag_total,
        channel: Channel::Video,
        flags: 0,
        payload,
    }
}

// ── VideoFragmenter ───────────────────────────────────────────────────────────

fn bench_video_fragmenter(c: &mut Criterion) {
    let sizes = [1_000, 10_000, 100_000];
    let mut group = c.benchmark_group("VideoFragmenter/fragment");

    for size in sizes {
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &sz| {
            let pkt = make_encoded_packet(sz, false);
            let mut fragmenter = VideoFragmenter::with_default_payload();
            b.iter(|| {
                let frags = fragmenter.fragment(black_box(&pkt));
                black_box(frags);
            });
        });
    }
    group.finish();

    let mut group2 = c.benchmark_group("VideoFragmenter/keyframe");
    let pkt = make_encoded_packet(50_000, true);
    group2.throughput(Throughput::Bytes(50_000));
    group2.bench_function("50KB_keyframe", |b| {
        let mut fragmenter = VideoFragmenter::with_default_payload();
        b.iter(|| {
            let frags = fragmenter.fragment(black_box(&pkt));
            black_box(frags);
        });
    });
    group2.finish();
}

// ── VideoReassembler ──────────────────────────────────────────────────────────

fn bench_video_reassembler(c: &mut Criterion) {
    let mut group = c.benchmark_group("VideoReassembler/ingest");

    // Single-fragment frames (common case)
    group.bench_function("single_fragment_frame", |b| {
        let mut reassembler = VideoReassembler::with_default_max();
        let mut frame_id: u32 = 0;
        b.iter(|| {
            let frag = make_fragment(frame_id, 0, 1, vec![0u8; MAX_FRAGMENT_PAYLOAD]);
            let result = reassembler.ingest(black_box(frag));
            black_box(result);
            frame_id = frame_id.wrapping_add(1);
        });
    });

    // 10-fragment frames
    group.bench_function("ten_fragment_frame", |b| {
        let mut reassembler = VideoReassembler::with_default_max();
        let mut frame_id: u32 = 0;
        b.iter(|| {
            for i in 0u16..10 {
                let frag = make_fragment(frame_id, i, 10, vec![0u8; MAX_FRAGMENT_PAYLOAD]);
                let result = reassembler.ingest(black_box(frag));
                black_box(result);
            }
            frame_id = frame_id.wrapping_add(1);
        });
    });

    group.finish();

    // evict_before benchmark
    let mut group3 = c.benchmark_group("VideoReassembler/evict_before");
    group3.bench_function("evict_4_frames", |b| {
        b.iter(|| {
            let mut reassembler = VideoReassembler::new(10);
            for id in 0u32..4 {
                reassembler.ingest(make_fragment(id, 0, 2, vec![]));
            }
            let count = reassembler.evict_before(black_box(4));
            black_box(count);
        });
    });
    group3.finish();
}

// ── Wire encode / decode ──────────────────────────────────────────────────────

fn bench_wire_encode_decode(c: &mut Criterion) {
    let payload_sizes = [0usize, 512, MAX_FRAGMENT_PAYLOAD];
    let mut group = c.benchmark_group("Wire/encode");

    for size in payload_sizes {
        group.throughput(Throughput::Bytes((size + 12) as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &sz| {
            let frag = VideoFragment {
                frame_id: 42,
                frag_index: 0,
                frag_total: 1,
                channel: Channel::Video,
                flags: FLAG_KEYFRAME,
                payload: vec![0xABu8; sz],
            };
            b.iter(|| {
                let encoded = frag.encode();
                black_box(encoded);
            });
        });
    }
    group.finish();

    let mut group2 = c.benchmark_group("Wire/decode");
    for size in payload_sizes {
        group2.throughput(Throughput::Bytes((size + 12) as u64));
        group2.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &sz| {
            let frag = VideoFragment {
                frame_id: 42,
                frag_index: 0,
                frag_total: 1,
                channel: Channel::Video,
                flags: 0,
                payload: vec![0xABu8; sz],
            };
            let encoded = frag.encode();
            b.iter(|| {
                let decoded = VideoFragment::decode(black_box(&encoded));
                black_box(decoded);
            });
        });
    }
    group2.finish();

    // Combined encode+decode round-trip
    let mut group3 = c.benchmark_group("Wire/roundtrip");
    group3.throughput(Throughput::Bytes((MAX_FRAGMENT_PAYLOAD + 12) as u64));
    group3.bench_function("full_payload", |b| {
        let frag = VideoFragment {
            frame_id: 1,
            frag_index: 0,
            frag_total: 1,
            channel: Channel::Video,
            flags: 0,
            payload: vec![0xFFu8; MAX_FRAGMENT_PAYLOAD],
        };
        b.iter(|| {
            let encoded = frag.encode();
            let decoded = VideoFragment::decode(black_box(&encoded)).expect("decode");
            black_box(decoded);
        });
    });
    group3.finish();
}

criterion_group!(
    benches,
    bench_video_fragmenter,
    bench_video_reassembler,
    bench_wire_encode_decode
);
criterion_main!(benches);
