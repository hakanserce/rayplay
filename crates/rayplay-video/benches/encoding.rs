//! Benchmarks for the video encoding pipeline (UC-002).
//!
//! These benchmarks measure the cross-platform components of the pipeline.
//! NVENC hardware encoding benchmarks run only on Windows with a supported GPU.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rayplay_video::{
    Bitrate, DEFAULT_CHUNK_SIZE, EncodedPacket, EncoderConfig, FrameChunker, RawFrame,
};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn make_raw_frame(width: u32, height: u32, timestamp_us: u64) -> RawFrame {
    let size = (width as usize) * (height as usize) * 4;
    RawFrame::new(vec![0u8; size], width, height, width * 4, timestamp_us)
}

fn make_encoded_packet(size: usize) -> EncodedPacket {
    EncodedPacket::new(vec![0xABu8; size], true, 0, 16_667)
}

// ── EncoderConfig::resolved_bitrate ───────────────────────────────────────────

fn bench_auto_bitrate(c: &mut Criterion) {
    let mut group = c.benchmark_group("auto_bitrate");

    for (label, w, h, fps) in [
        ("1080p60", 1920u32, 1080u32, 60u32),
        ("4k60", 3840, 2160, 60),
        ("1440p144", 2560, 1440, 144),
    ] {
        group.bench_with_input(
            BenchmarkId::new("resolve", label),
            &(w, h, fps),
            |b, &(w, h, f)| {
                let cfg = EncoderConfig::new(w, h, f);
                b.iter(|| black_box(cfg.resolved_bitrate()))
            },
        );
    }

    group.finish();
}

// ── FrameChunker throughput ────────────────────────────────────────────────────

fn bench_frame_chunker(c: &mut Criterion) {
    let mut group = c.benchmark_group("frame_chunker");

    // Typical encoded frame sizes for 1080p60 HEVC and 4K60 HEVC
    for (label, packet_bytes) in [
        ("1080p_keyframe_~150kB", 150_000usize),
        ("1080p_pframe_~20kB", 20_000),
        ("4k_keyframe_~600kB", 600_000),
        ("4k_pframe_~80kB", 80_000),
    ] {
        group.throughput(Throughput::Bytes(packet_bytes as u64));
        group.bench_with_input(
            BenchmarkId::new("chunk", label),
            &packet_bytes,
            |b, &size| {
                let packet = make_encoded_packet(size);
                let mut chunker = FrameChunker::new(DEFAULT_CHUNK_SIZE);
                b.iter(|| black_box(chunker.chunk(&packet)))
            },
        );
    }

    group.finish();
}

// ── RawFrame allocation ────────────────────────────────────────────────────────

fn bench_raw_frame_alloc(c: &mut Criterion) {
    let mut group = c.benchmark_group("raw_frame");

    for (label, w, h) in [("1080p", 1920u32, 1080u32), ("4k", 3840, 2160)] {
        let pixels = (w as usize) * (h as usize) * 4;
        group.throughput(Throughput::Bytes(pixels as u64));
        group.bench_with_input(BenchmarkId::new("new", label), &(w, h), |b, &(w, h)| {
            b.iter(|| black_box(make_raw_frame(w, h, 0)))
        });
    }

    group.finish();
}

// ── Bitrate::Auto resolution ───────────────────────────────────────────────────

fn bench_bitrate_resolve(c: &mut Criterion) {
    c.bench_function("bitrate_auto_resolve_1080p60", |b| {
        b.iter(|| black_box(Bitrate::Auto.resolve(1920, 1080, 60)))
    });
}

// ── NVENC zero-copy benchmarks (Windows + hw-codec-tests only) ────────────────

#[cfg(all(target_os = "windows", feature = "hw-codec-tests"))]
fn bench_nvenc_zero_copy(c: &mut Criterion, label: &str, width: u32, height: u32) {
    use std::sync::Arc;

    use rayplay_video::{
        CaptureConfig, EncoderInput, GpuTextureHandle, SharedD3D11Device,
        capture::ZeroCopyCapturer, dxgi_capture::DxgiCapture, nvenc::NvencEncoder,
    };

    let device = Arc::new(SharedD3D11Device::new().expect("D3D11 device"));
    let cap_config = CaptureConfig {
        target_fps: 60,
        acquire_timeout_ms: 100,
    };
    let capturer = DxgiCapture::new(cap_config, device.clone()).expect("DXGI capture");
    let enc_config = EncoderConfig::new(width, height, 60);
    let mut encoder = NvencEncoder::new(enc_config).expect("NVENC encoder");

    c.bench_function(label, |b| {
        b.iter(|| {
            let texture = capturer.acquire_texture().expect("acquire");
            let input = EncoderInput::GpuTexture {
                handle: GpuTextureHandle(texture.texture_ptr),
                width: texture.width,
                height: texture.height,
                timestamp_us: 0,
            };
            let result = encoder.encode(input);
            capturer.release_frame();
            black_box(result)
        });
    });
}

#[cfg(all(target_os = "windows", feature = "hw-codec-tests"))]
fn bench_nvenc_zero_copy_1080p60(c: &mut Criterion) {
    bench_nvenc_zero_copy(c, "nvenc_zero_copy_1080p60", 1920, 1080);
}

#[cfg(all(target_os = "windows", feature = "hw-codec-tests"))]
fn bench_nvenc_zero_copy_4k60(c: &mut Criterion) {
    bench_nvenc_zero_copy(c, "nvenc_zero_copy_4k60", 3840, 2160);
}

criterion_group!(
    benches,
    bench_auto_bitrate,
    bench_frame_chunker,
    bench_raw_frame_alloc,
    bench_bitrate_resolve,
);
criterion_main!(benches);
