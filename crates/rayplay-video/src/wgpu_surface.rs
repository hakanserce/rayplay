//! Surface-backed `WgpuRenderer` constructor (UC-005, ADR-005).
//!
//! This module contains only [`WgpuRenderer::new`], which initialises the
//! GPU renderer for a live `winit` window.
//!
//! # Coverage exclusion
//!
//! This file is excluded from the workspace line-coverage gate via
//! `--ignore-filename-regex` in `coverage-ci` (see `Makefile.toml`).
//! Every line requires a `wgpu::Surface` backed by a real OS window —
//! a resource that cannot be fabricated in unit tests:
//!
//! * [`wgpu::Instance::create_surface`] demands a valid `WindowHandle`.
//! * [`wgpu::Adapter`] selection uses `compatible_surface`, which is
//!   only available once a surface exists.
//! * [`wgpu::Surface::configure`] and [`wgpu::Surface::get_current_texture`]
//!   require a running compositor / swap chain.
//!
//! The surface path is covered under the `hw-render-tests` feature flag on
//! hardware CI runners that have a live display.  All other GPU code lives in
//! `wgpu_renderer.rs` and is tested via the headless offscreen path.

use std::sync::Arc;

use winit::window::Window;

use crate::{
    renderer::RenderError,
    wgpu_renderer::{RendererOutput, WgpuRenderer, select_present_mode, select_surface_format},
};

impl WgpuRenderer {
    /// Initialises the GPU renderer for the given `winit` window.
    ///
    /// Selects the best available GPU adapter (high-performance preference),
    /// configures the swap chain with `Mailbox` present mode when available
    /// (falls back to `Fifo`), and compiles the BGRA8 and NV12 render
    /// pipelines.
    ///
    /// # Errors
    ///
    /// - [`RenderError::NoAdapter`] — no Metal-capable GPU found.
    /// - [`RenderError::Failed`] — surface creation or device init failed.
    pub async fn new(window: Arc<Window>) -> Result<Self, RenderError> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            // Prefer the native backend for the current platform.  On macOS
            // this pins to Metal rather than letting wgpu probe Vulkan/DX12
            // fallbacks that are unavailable anyway.
            backends: if cfg!(target_os = "macos") {
                wgpu::Backends::METAL
            } else {
                wgpu::Backends::all()
            },
            ..Default::default()
        });
        let surface =
            instance
                .create_surface(Arc::clone(&window))
                .map_err(|e| RenderError::Failed {
                    reason: format!("create_surface: {e}"),
                })?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or(RenderError::NoAdapter)?;
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("rayplay_device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::default(),
                },
                None,
            )
            .await
            .map_err(|e| RenderError::Failed {
                reason: format!("request_device: {e}"),
            })?;
        let caps = surface.get_capabilities(&adapter);
        let format = select_surface_format(&caps.formats);
        let present_mode = select_present_mode(&caps.present_modes);
        let alpha_mode = caps
            .alpha_modes
            .first()
            .copied()
            .unwrap_or(wgpu::CompositeAlphaMode::Auto);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);
        let output = RendererOutput::Surface { surface, config };
        Ok(Self::from_parts(device, queue, output))
    }
}
