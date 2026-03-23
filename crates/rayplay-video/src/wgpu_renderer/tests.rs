use super::*;
use crate::{DecodedFrame, PixelFormat};

// ── Headless GPU helper ────────────────────────────────────────────────────

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
            .expect("no GPU adapter — run with a Metal-capable device");
        adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
            .expect("device creation failed")
    })
}

fn make_offscreen(width: u32, height: u32) -> WgpuRenderer {
    let (device, queue) = create_headless_device();
    WgpuRenderer::new_offscreen(device, queue, width, height)
}

fn make_bgra_frame(width: u32, height: u32) -> DecodedFrame {
    let stride = width * 4;
    // Use 0x80 (mid-grey) to produce valid BGRA data
    DecodedFrame::new_cpu(
        vec![0x80u8; (stride * height) as usize],
        width,
        height,
        stride,
        PixelFormat::Bgra8,
        0,
    )
}

fn make_nv12_frame(width: u32, height: u32) -> DecodedFrame {
    let stride = width;
    let size = (stride * height * 3 / 2) as usize;
    DecodedFrame::new_cpu(
        vec![0x80u8; size],
        width,
        height,
        stride,
        PixelFormat::Nv12,
        0,
    )
}

// ── select_surface_format ─────────────────────────────────────────────────

#[test]
fn test_select_surface_format_prefers_bgra8() {
    let formats = vec![
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Bgra8Unorm,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    ];
    assert_eq!(
        select_surface_format(&formats),
        wgpu::TextureFormat::Bgra8Unorm
    );
}

#[test]
fn test_select_surface_format_fallback_to_first() {
    let formats = vec![
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Rgba8UnormSrgb,
    ];
    assert_eq!(
        select_surface_format(&formats),
        wgpu::TextureFormat::Rgba8Unorm
    );
}

#[test]
fn test_select_surface_format_empty_slice_defaults_to_bgra8() {
    assert_eq!(select_surface_format(&[]), wgpu::TextureFormat::Bgra8Unorm);
}

#[test]
fn test_select_surface_format_single_bgra8() {
    let formats = vec![wgpu::TextureFormat::Bgra8Unorm];
    assert_eq!(
        select_surface_format(&formats),
        wgpu::TextureFormat::Bgra8Unorm
    );
}

// ── select_present_mode ───────────────────────────────────────────────────

#[test]
fn test_select_present_mode_prefers_mailbox() {
    let modes = vec![wgpu::PresentMode::Fifo, wgpu::PresentMode::Mailbox];
    assert_eq!(select_present_mode(&modes), wgpu::PresentMode::Mailbox);
}

#[test]
fn test_select_present_mode_falls_back_to_fifo() {
    let modes = vec![wgpu::PresentMode::Fifo, wgpu::PresentMode::Immediate];
    assert_eq!(select_present_mode(&modes), wgpu::PresentMode::Fifo);
}

#[test]
fn test_select_present_mode_only_mailbox() {
    let modes = vec![wgpu::PresentMode::Mailbox];
    assert_eq!(select_present_mode(&modes), wgpu::PresentMode::Mailbox);
}

#[test]
fn test_select_present_mode_empty_defaults_to_fifo() {
    assert_eq!(select_present_mode(&[]), wgpu::PresentMode::Fifo);
}

// ── surface_error_to_render_error ─────────────────────────────────────────

#[test]
fn test_surface_error_lost_maps_to_surface_lost() {
    let err = surface_error_to_render_error(&wgpu::SurfaceError::Lost);
    assert!(matches!(err, RenderError::SurfaceLost));
}

#[test]
fn test_surface_error_outdated_maps_to_surface_lost() {
    let err = surface_error_to_render_error(&wgpu::SurfaceError::Outdated);
    assert!(matches!(err, RenderError::SurfaceLost));
}

#[test]
fn test_surface_error_oom_maps_to_failed() {
    let err = surface_error_to_render_error(&wgpu::SurfaceError::OutOfMemory);
    assert!(matches!(err, RenderError::Failed { .. }));
    assert!(err.to_string().contains("memory"));
}

#[test]
fn test_surface_error_timeout_maps_to_failed() {
    let err = surface_error_to_render_error(&wgpu::SurfaceError::Timeout);
    assert!(matches!(err, RenderError::Failed { .. }));
    assert!(err.to_string().contains("timeout"));
}

// ── WGSL shader source ────────────────────────────────────────────────────

#[test]
fn test_bgra_shader_contains_vs_and_fs_main() {
    assert!(BGRA_SHADER.contains("fn vs_main"));
    assert!(BGRA_SHADER.contains("fn fs_main"));
}

#[test]
fn test_nv12_shader_contains_vs_and_fs_main() {
    assert!(NV12_SHADER.contains("fn vs_main"));
    assert!(NV12_SHADER.contains("fn fs_main"));
}

#[test]
fn test_nv12_shader_has_yuv_conversion() {
    assert!(NV12_SHADER.contains("y_val"));
    assert!(NV12_SHADER.contains("uv_val"));
}

// ── RendererOutput::surface_format ────────────────────────────────────────

#[test]
fn test_offscreen_surface_format_is_rgba8() {
    let (device, queue) = create_headless_device();
    let r = WgpuRenderer::new_offscreen(device, queue, 64, 64);
    // surface_format() on an Offscreen variant delegates to the texture format.
    let RendererOutput::Offscreen { texture } = &r.output else {
        panic!("expected offscreen output")
    };
    assert_eq!(texture.format(), wgpu::TextureFormat::Rgba8Unorm);
}

// ── new_offscreen construction ────────────────────────────────────────────

#[test]
fn test_new_offscreen_creates_renderer() {
    let _r = make_offscreen(64, 64);
}

#[test]
fn test_new_offscreen_zero_dimensions_clamped_to_one() {
    let (device, queue) = create_headless_device();
    let r = WgpuRenderer::new_offscreen(device, queue, 0, 0);
    let RendererOutput::Offscreen { texture } = &r.output else {
        unreachable!("new_offscreen always produces Offscreen output")
    };
    assert_eq!(texture.size().width, 1);
    assert_eq!(texture.size().height, 1);
}

// ── texture_matches ───────────────────────────────────────────────────────

#[test]
fn test_texture_matches_false_when_no_cache() {
    let r = make_offscreen(64, 64);
    let frame = make_bgra_frame(64, 64);
    assert!(!r.texture_matches(&frame));
}

#[test]
fn test_texture_matches_true_after_present_bgra() {
    let mut r = make_offscreen(64, 64);
    let frame = make_bgra_frame(64, 64);
    r.present_frame(&frame).unwrap();
    assert!(r.texture_matches(&frame));
}

#[test]
fn test_texture_matches_true_after_present_nv12() {
    let mut r = make_offscreen(64, 64);
    let frame = make_nv12_frame(64, 64);
    r.present_frame(&frame).unwrap();
    assert!(r.texture_matches(&frame));
}

#[test]
fn test_texture_matches_false_when_dimensions_change() {
    let mut r = make_offscreen(128, 128);
    let frame_a = make_bgra_frame(64, 64);
    r.present_frame(&frame_a).unwrap();
    let frame_b = make_bgra_frame(128, 128);
    assert!(!r.texture_matches(&frame_b));
}

#[test]
fn test_texture_matches_false_when_format_changes() {
    let mut r = make_offscreen(64, 64);
    let bgra = make_bgra_frame(64, 64);
    r.present_frame(&bgra).unwrap();
    let nv12 = make_nv12_frame(64, 64);
    assert!(!r.texture_matches(&nv12));
}

// ── present_frame (offscreen path) ────────────────────────────────────────

#[test]
fn test_present_bgra_frame_succeeds() {
    let mut r = make_offscreen(64, 64);
    let frame = make_bgra_frame(64, 64);
    assert!(r.present_frame(&frame).is_ok());
}

#[test]
fn test_present_nv12_frame_succeeds() {
    let mut r = make_offscreen(64, 64);
    let frame = make_nv12_frame(64, 64);
    assert!(r.present_frame(&frame).is_ok());
}

#[test]
fn test_present_multiple_bgra_frames_reuses_cache() {
    let mut r = make_offscreen(64, 64);
    let frame = make_bgra_frame(64, 64);
    r.present_frame(&frame).unwrap();
    r.present_frame(&frame).unwrap();
    r.present_frame(&frame).unwrap();
    assert!(r.texture_matches(&frame));
}

#[test]
fn test_present_bgra_then_nv12_recreates_cache() {
    let mut r = make_offscreen(64, 64);
    let bgra = make_bgra_frame(64, 64);
    r.present_frame(&bgra).unwrap();
    let nv12 = make_nv12_frame(64, 64);
    r.present_frame(&nv12).unwrap();
    assert!(r.texture_matches(&nv12));
}

#[test]
fn test_present_frame_1080p_bgra() {
    let mut r = make_offscreen(1920, 1080);
    let frame = make_bgra_frame(1920, 1080);
    assert!(r.present_frame(&frame).is_ok());
}

// ── resize (offscreen — no-op) ────────────────────────────────────────────

#[test]
fn test_resize_offscreen_is_noop() {
    let mut r = make_offscreen(64, 64);
    // Should not panic or error
    r.resize(winit::dpi::PhysicalSize::new(128, 128));
}

// ── build_bgra_pipeline / build_nv12_pipeline ─────────────────────────────

#[test]
fn test_bgra_pipeline_compiles() {
    let (device, _queue) = create_headless_device();
    let (_bgl, _pipeline) =
        build_bgra_pipeline(&device, wgpu::TextureFormat::Rgba8Unorm, BGRA_SHADER);
}

#[test]
fn test_nv12_pipeline_compiles() {
    let (device, _queue) = create_headless_device();
    let (_nv12_bgl, _pipeline) =
        build_nv12_pipeline(&device, wgpu::TextureFormat::Rgba8Unorm, NV12_SHADER);
}

// ── upload_frame with no cache ────────────────────────────────────────────

#[test]
fn test_upload_frame_no_op_when_no_cache() {
    // With texture_cache == None, upload_frame must hit the `_ => {}` arm without panicking.
    let r = make_offscreen(64, 64);
    let frame = DecodedFrame::new_cpu(vec![0u8; 4], 1, 1, 4, PixelFormat::Bgra8, 0);
    r.upload_frame(&frame);
}

// ── present_frame: hardware frame fallback (no IOSurface) ───────────────

#[test]
fn test_present_hardware_frame_without_iosurface_succeeds() {
    let mut r = make_offscreen(64, 64);
    let frame = DecodedFrame::new_hardware_test_stub(64, 64, 64, PixelFormat::Nv12, 0);
    // Should succeed with a clear-only render (no crash, no panic).
    assert!(r.present_frame(&frame).is_ok());
}
