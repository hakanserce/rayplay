//! Benchmarks for the video decoding pipeline (UC-004).
//!
//! Measures cross-platform components of the decode path using a `NullDecoder`
//! baseline. Hardware-accelerated `VtDecoder` benchmarks require Apple Silicon
//! and run only when `--features hw-codec-tests` is passed.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rayplay_video::{DecodedFrame, EncodedPacket, PixelFormat};

// ── Helpers ────────────────────────────────────────────────────────────────────

fn make_encoded_packet(size: usize, is_keyframe: bool) -> EncodedPacket {
    EncodedPacket::new(vec![0xABu8; size], is_keyframe, 0, 16_667)
}

fn make_decoded_frame_cpu(width: u32, height: u32) -> DecodedFrame {
    let stride = width * 4;
    let size = (stride * height) as usize;
    DecodedFrame::new_cpu(
        vec![0u8; size],
        width,
        height,
        stride,
        PixelFormat::Bgra8,
        0,
    )
}

// ── EncodedPacket construction throughput ─────────────────────────────────────

fn bench_encoded_packet_construction(c: &mut Criterion) {
    let mut group = c.benchmark_group("encoded_packet_construction");

    for (label, packet_bytes) in [
        ("1080p_keyframe_~150kB", 150_000usize),
        ("1080p_pframe_~20kB", 20_000),
        ("4k_keyframe_~600kB", 600_000),
        ("4k_pframe_~80kB", 80_000),
    ] {
        group.throughput(Throughput::Bytes(packet_bytes as u64));
        group.bench_with_input(BenchmarkId::new("new", label), &packet_bytes, |b, &size| {
            b.iter(|| black_box(make_encoded_packet(size, false)))
        });
    }

    group.finish();
}

// ── DecodedFrame CPU allocation ────────────────────────────────────────────────

fn bench_decoded_frame_cpu_alloc(c: &mut Criterion) {
    let mut group = c.benchmark_group("decoded_frame_cpu_alloc");

    for (label, w, h) in [("1080p", 1920u32, 1080u32), ("4k", 3840, 2160)] {
        let pixels = (w as usize) * (h as usize) * 4;
        group.throughput(Throughput::Bytes(pixels as u64));
        group.bench_with_input(BenchmarkId::new("new_cpu", label), &(w, h), |b, &(w, h)| {
            b.iter(|| black_box(make_decoded_frame_cpu(w, h)))
        });
    }

    group.finish();
}

// ── DecodedFrame::expected_data_size ──────────────────────────────────────────

fn bench_expected_data_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("decoded_frame_expected_data_size");

    for (label, w, h, fmt) in [
        ("bgra8_1080p", 1920u32, 1080u32, PixelFormat::Bgra8),
        ("nv12_1080p", 1920, 1080, PixelFormat::Nv12),
        ("bgra8_4k", 3840, 2160, PixelFormat::Bgra8),
        ("nv12_4k", 3840, 2160, PixelFormat::Nv12),
    ] {
        group.bench_with_input(
            BenchmarkId::new("size", label),
            &(w, h, fmt),
            |b, (w, h, fmt)| {
                let frame = DecodedFrame::new_cpu(vec![], *w, *h, *w * 4, fmt.clone(), 0);
                b.iter(|| black_box(frame.expected_data_size()))
            },
        );
    }

    group.finish();
}

// ── OpenH264 software decode (fallback feature) ─────────────────────────────

#[cfg(feature = "fallback")]
fn bench_openh264_decode(c: &mut Criterion) {
    use rayplay_video::decoder::VideoDecoder;
    use rayplay_video::encoder::{EncoderInput, VideoEncoder};
    use rayplay_video::{Codec, EncoderConfig, OpenH264Decoder, OpenH264Encoder, RawFrame};

    let mut group = c.benchmark_group("openh264_decode");

    for (label, w, h) in [("720p", 1280u32, 720u32), ("480p", 640, 480)] {
        // Encode a frame first to get a valid bitstream for decoding.
        let config = EncoderConfig::with_codec(w, h, 30, Codec::H264);
        let mut encoder = OpenH264Encoder::new(config).expect("OpenH264Encoder");
        let size = (w as usize) * (h as usize) * 4;
        let frame = RawFrame::new(vec![128u8; size], w, h, w * 4, 0);
        let packet = encoder
            .encode(EncoderInput::Cpu(&frame))
            .expect("encode")
            .expect("packet");

        let pixels = (w as usize) * (h as usize) * 4;
        group.throughput(Throughput::Bytes(pixels as u64));
        group.bench_with_input(BenchmarkId::new("decode", label), &packet, |b, packet| {
            let mut decoder = OpenH264Decoder::new(Codec::H264).expect("OpenH264Decoder");
            b.iter(|| std::hint::black_box(decoder.decode(packet)));
        });
    }

    group.finish();
}

// ── FFmpeg software decode (ffmpeg-fallback feature) ─────────────────────────

#[cfg(feature = "ffmpeg-fallback")]
fn bench_ffmpeg_decode(c: &mut Criterion) {
    use rayplay_video::decoder::VideoDecoder;
    use rayplay_video::encoder::{EncoderInput, VideoEncoder};
    use rayplay_video::{Codec, EncoderConfig, FfmpegDecoder, FfmpegEncoder, RawFrame};

    let mut group = c.benchmark_group("ffmpeg_decode");

    for (label, w, h, codec) in [
        ("h264_720p", 1280u32, 720u32, Codec::H264),
        ("h264_480p", 640, 480, Codec::H264),
        ("hevc_720p", 1280, 720, Codec::Hevc),
    ] {
        let config = EncoderConfig::with_codec(w, h, 30, codec);
        let mut encoder = FfmpegEncoder::new(config).expect("FfmpegEncoder");
        let size = (w as usize) * (h as usize) * 4;
        let frame = RawFrame::new(vec![128u8; size], w, h, w * 4, 0);

        // Encode multiple frames to get output
        let mut packets = Vec::new();
        for _ in 0..5 {
            if let Some(pkt) = encoder.encode(EncoderInput::Cpu(&frame)).expect("encode") {
                packets.push(pkt);
            }
        }
        packets.extend(encoder.flush().expect("flush"));

        if let Some(packet) = packets.into_iter().next() {
            let pixels = (w as usize) * (h as usize) * 4;
            group.throughput(Throughput::Bytes(pixels as u64));
            group.bench_with_input(BenchmarkId::new("decode", label), &packet, |b, packet| {
                let mut decoder = FfmpegDecoder::new(codec).expect("FfmpegDecoder");
                b.iter(|| std::hint::black_box(decoder.decode(packet)));
            });
        }
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_encoded_packet_construction,
    bench_decoded_frame_cpu_alloc,
    bench_expected_data_size,
);

#[cfg(feature = "fallback")]
criterion_group!(fallback_benches, bench_openh264_decode,);

#[cfg(feature = "ffmpeg-fallback")]
criterion_group!(ffmpeg_benches, bench_ffmpeg_decode,);

#[cfg(all(feature = "fallback", feature = "ffmpeg-fallback"))]
criterion_main!(benches, fallback_benches, ffmpeg_benches);
#[cfg(all(feature = "fallback", not(feature = "ffmpeg-fallback")))]
criterion_main!(benches, fallback_benches);
#[cfg(all(not(feature = "fallback"), feature = "ffmpeg-fallback"))]
criterion_main!(benches, ffmpeg_benches);
#[cfg(not(any(feature = "fallback", feature = "ffmpeg-fallback")))]
criterion_main!(benches);
