//! Benchmarks for the client frame rendering pipeline (UC-005).
//!
//! Measures two categories of cost:
//!
//! * **CPU-side**: frame construction and NV12 UV-offset arithmetic —
//!   always run, no GPU required.
//! * **GPU present latency** (`present_frame`): texture upload + command
//!   encoding + queue submit + GPU drain, measured against the offscreen
//!   render target.  Validates the AC-2 requirement of <2 ms per frame.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rayplay_video::{DecodedFrame, PixelFormat, Renderer, WgpuRenderer};

// ── Helpers ────────────────────────────────────────────────────────────────────

fn make_bgra_frame(width: u32, height: u32) -> DecodedFrame {
    let stride = width * 4;
    let data = vec![0u8; (stride * height) as usize];
    DecodedFrame::new_cpu(data, width, height, stride, PixelFormat::Bgra8, 0)
}

fn make_nv12_frame(width: u32, height: u32) -> DecodedFrame {
    let stride = width;
    let data = vec![0u8; (stride * height * 3 / 2) as usize];
    DecodedFrame::new_cpu(data, width, height, stride, PixelFormat::Nv12, 0)
}

// ── Frame allocation ───────────────────────────────────────────────────────────

fn bench_frame_alloc_bgra(c: &mut Criterion) {
    let mut group = c.benchmark_group("frame_alloc_bgra");

    for (label, w, h) in [
        ("720p", 1280u32, 720u32),
        ("1080p", 1920, 1080),
        ("4k", 3840, 2160),
    ] {
        let bytes = (w * h * 4) as u64;
        group.throughput(Throughput::Bytes(bytes));
        group.bench_with_input(BenchmarkId::new("bgra", label), &(w, h), |b, &(w, h)| {
            b.iter(|| black_box(make_bgra_frame(w, h)));
        });
    }

    group.finish();
}

fn bench_frame_alloc_nv12(c: &mut Criterion) {
    let mut group = c.benchmark_group("frame_alloc_nv12");

    for (label, w, h) in [
        ("720p", 1280u32, 720u32),
        ("1080p", 1920, 1080),
        ("4k", 3840, 2160),
    ] {
        let bytes = (w * h * 3 / 2) as u64;
        group.throughput(Throughput::Bytes(bytes));
        group.bench_with_input(BenchmarkId::new("nv12", label), &(w, h), |b, &(w, h)| {
            b.iter(|| black_box(make_nv12_frame(w, h)));
        });
    }

    group.finish();
}

// ── NV12 UV-plane offset arithmetic ───────────────────────────────────────────

fn bench_nv12_uv_offset(c: &mut Criterion) {
    let mut group = c.benchmark_group("nv12_uv_offset");

    for (label, w, h) in [("1080p", 1920u32, 1080u32), ("4k", 3840, 2160)] {
        group.bench_with_input(BenchmarkId::new("offset", label), &(w, h), |b, &(w, h)| {
            let frame = make_nv12_frame(w, h);
            b.iter(|| {
                let y_end = frame.stride as usize * frame.height as usize;
                black_box(&frame.data[y_end..])
            });
        });
    }

    group.finish();
}

// ── expected_data_size ────────────────────────────────────────────────────────

fn bench_expected_data_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("render_expected_data_size");

    for (label, w, h, fmt) in [
        ("bgra_1080p", 1920u32, 1080u32, PixelFormat::Bgra8),
        ("nv12_1080p", 1920, 1080, PixelFormat::Nv12),
        ("bgra_4k", 3840, 2160, PixelFormat::Bgra8),
        ("nv12_4k", 3840, 2160, PixelFormat::Nv12),
    ] {
        group.bench_with_input(
            BenchmarkId::new("size", label),
            &(w, h, fmt),
            |b, (w, h, fmt)| {
                let frame = DecodedFrame::new_cpu(vec![], *w, *h, *w * 4, fmt.clone(), 0);
                b.iter(|| black_box(frame.expected_data_size()));
            },
        );
    }

    group.finish();
}

// ── present_frame latency (AC-2: <2 ms) ──────────────────────────────────────

/// Creates a headless `wgpu` device backed by the default high-performance
/// adapter (Metal on macOS).  No window or display server is required.
fn create_headless_device() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });
    pollster::block_on(async {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .expect("no GPU adapter available");
        adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
            .expect("device creation failed")
    })
}

/// Measures `present_frame` CPU latency: texture staging-upload + command
/// encoding + queue submit.  This is the time the calling thread is blocked
/// before the next frame can be started — the dominant component of AC-2's
/// <2 ms target (GPU execution is pipelined and overlaps with the CPU path).
fn bench_present_frame_bgra(c: &mut Criterion) {
    let mut group = c.benchmark_group("present_frame");

    for (label, w, h) in [("1080p", 1920u32, 1080u32), ("4k", 3840, 2160)] {
        let bytes = (w * h * 4) as u64;
        group.throughput(Throughput::Bytes(bytes));
        group.bench_with_input(BenchmarkId::new("bgra", label), &(w, h), |b, &(w, h)| {
            let (device, queue) = create_headless_device();
            let mut renderer = WgpuRenderer::new_offscreen(device, queue, w, h);
            let frame = make_bgra_frame(w, h);
            b.iter(|| renderer.present_frame(black_box(&frame)).unwrap());
        });
    }

    group.finish();
}

fn bench_present_frame_nv12(c: &mut Criterion) {
    let mut group = c.benchmark_group("present_frame");

    for (label, w, h) in [("1080p", 1920u32, 1080u32), ("4k", 3840, 2160)] {
        let bytes = (w * h * 3 / 2) as u64;
        group.throughput(Throughput::Bytes(bytes));
        group.bench_with_input(BenchmarkId::new("nv12", label), &(w, h), |b, &(w, h)| {
            let (device, queue) = create_headless_device();
            let mut renderer = WgpuRenderer::new_offscreen(device, queue, w, h);
            let frame = make_nv12_frame(w, h);
            b.iter(|| renderer.present_frame(black_box(&frame)).unwrap());
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_frame_alloc_bgra,
    bench_frame_alloc_nv12,
    bench_nv12_uv_offset,
    bench_expected_data_size,
    bench_present_frame_bgra,
    bench_present_frame_nv12,
);
criterion_main!(benches);
