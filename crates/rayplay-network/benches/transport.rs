use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rayplay_core::packet::EncodedPacket;
use rayplay_network::{
    FLAG_KEYFRAME, MAX_FRAGMENT_PAYLOAD, VideoFragment, fragmenter::FrameFragmenter,
    reassembler::FrameReassembler, wire::Channel,
};

fn make_encoded_packet(size: usize, is_keyframe: bool) -> EncodedPacket {
    EncodedPacket::new(vec![0xAAu8; size], is_keyframe, 42, 16_667)
}

fn make_video_fragment(
    frame_id: u32,
    frag_index: u16,
    frag_total: u16,
    payload_size: usize,
) -> VideoFragment {
    VideoFragment {
        frame_id,
        frag_index,
        frag_total,
        channel: Channel::Video,
        flags: 0,
        payload: vec![0xBBu8; payload_size],
    }
}

fn bench_fragmenter(c: &mut Criterion) {
    let mut group = c.benchmark_group("fragmenter");
    let fragmenter = FrameFragmenter::new();

    for &size in &[1024, 4096, 16384, 65536] {
        group.throughput(Throughput::Bytes(size as u64));
        let packet = make_encoded_packet(size, false);

        group.bench_with_input(BenchmarkId::new("fragment", size), &packet, |b, pkt| {
            b.iter(|| {
                let frags: Vec<_> = fragmenter.fragment(black_box(pkt)).collect();
                black_box(frags);
            });
        });
    }
    group.finish();
}

fn bench_reassembler(c: &mut Criterion) {
    let mut group = c.benchmark_group("reassembler");

    for &size in &[1024, 4096, 16384, 65536] {
        group.throughput(Throughput::Bytes(size as u64));

        let num_fragments = (size + MAX_FRAGMENT_PAYLOAD - 1) / MAX_FRAGMENT_PAYLOAD;
        let mut fragments = Vec::new();

        for i in 0..num_fragments {
            let payload_size = std::cmp::min(MAX_FRAGMENT_PAYLOAD, size - i * MAX_FRAGMENT_PAYLOAD);
            fragments.push(make_video_fragment(
                42,
                i as u16,
                num_fragments as u16,
                payload_size,
            ));
        }

        group.bench_with_input(
            BenchmarkId::new("reassemble", size),
            &fragments,
            |b, frags| {
                b.iter(|| {
                    let mut reassembler = FrameReassembler::new();
                    for frag in frags.iter().cloned() {
                        if let Some(packet) = reassembler.add_fragment(black_box(frag)) {
                            black_box(packet);
                        }
                    }
                });
            },
        );
    }
    group.finish();
}

fn bench_wire_encoding(c: &mut Criterion) {
    let mut group = c.benchmark_group("wire_encoding");

    for &payload_size in &[64, 256, 1024, MAX_FRAGMENT_PAYLOAD] {
        group.throughput(Throughput::Bytes(payload_size as u64));
        let fragment = make_video_fragment(123, 0, 1, payload_size);

        group.bench_with_input(
            BenchmarkId::new("encode", payload_size),
            &fragment,
            |b, frag| {
                b.iter(|| {
                    let encoded = frag.encode();
                    black_box(encoded);
                });
            },
        );
    }
    group.finish();
}

fn bench_wire_decoding(c: &mut Criterion) {
    let mut group = c.benchmark_group("wire_decoding");

    for &payload_size in &[64, 256, 1024, MAX_FRAGMENT_PAYLOAD] {
        group.throughput(Throughput::Bytes(payload_size as u64));
        let fragment = make_video_fragment(123, 0, 1, payload_size);
        let encoded = fragment.encode();

        group.bench_with_input(
            BenchmarkId::new("decode", payload_size),
            &encoded,
            |b, data| {
                b.iter(|| {
                    let decoded = VideoFragment::decode(black_box(data)).unwrap();
                    black_box(decoded);
                });
            },
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_fragmenter,
    bench_reassembler,
    bench_wire_encoding,
    bench_wire_decoding
);
criterion_main!(benches);
