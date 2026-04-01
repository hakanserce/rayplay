//! Surface-backed `WgpuRenderer` constructor and surface-specific operations
//! (UC-005, ADR-005).
//!
//! This module contains [`WgpuRenderer::new`] (surface renderer constructor),
//! [`WgpuRenderer::apply_resize`] (swap-chain resize), and
//! [`WgpuRenderer::present_to_surface`] (surface frame presentation).
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
    wgpu_renderer::{
        RendererOutput, WgpuRenderer, select_present_mode, select_surface_format,
        surface_error_to_render_error,
    },
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
        Ok(Self::from_parts(device, queue, output, format))
    }

    /// Reconfigures the swap chain after a window resize.
    ///
    /// No-op when `self.output` is `Offscreen`.
    pub(crate) fn apply_resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if let RendererOutput::Surface { surface, config } = &mut self.output {
            config.width = new_size.width.max(1);
            config.height = new_size.height.max(1);
            surface.configure(&self.device, &*config);
        }
    }

    /// Acquires the next swap-chain frame, encodes, and presents it.
    ///
    /// Called from [`WgpuRenderer::present_frame`] after the offscreen path
    /// has already returned — so `self.output` is always `Surface` here.
    pub(crate) fn present_to_surface(
        &mut self,
        hw_bind_group: Option<&wgpu::BindGroup>,
    ) -> Result<(), RenderError> {
        let RendererOutput::Surface { surface, .. } = &self.output else {
            return Ok(());
        };
        let output = surface
            .get_current_texture()
            .map_err(|e| surface_error_to_render_error(&e))?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let cmd = self.encode_frame(&view, hw_bind_group);
        self.queue.submit(std::iter::once(cmd));
        output.present();
        Ok(())
    }

    /// Presents video frame + egui overlay in two render passes on the same
    /// surface texture.
    #[cfg(feature = "gui")]
    pub(crate) fn present_to_surface_with_egui(
        &mut self,
        hw_bind_group: Option<&wgpu::BindGroup>,
        egui_renderer: &mut egui_wgpu::Renderer,
        egui_primitives: &[egui::ClippedPrimitive],
        egui_textures_delta: &egui::TexturesDelta,
        screen_descriptor: &egui_wgpu::ScreenDescriptor,
    ) -> Result<(), RenderError> {
        let RendererOutput::Surface { surface, .. } = &self.output else {
            return Ok(());
        };
        let output = surface
            .get_current_texture()
            .map_err(|e| surface_error_to_render_error(&e))?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // Pass 1: video frame
        let video_cmd = self.encode_frame(&view, hw_bind_group);

        // Pass 2: egui overlay
        for (id, image_delta) in &egui_textures_delta.set {
            egui_renderer.update_texture(&self.device, &self.queue, *id, image_delta);
        }

        // Encode egui buffer updates (separate encoder to avoid lifetime issues)
        let mut buf_encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("egui_buffers"),
            });
        egui_renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut buf_encoder,
            egui_primitives,
            screen_descriptor,
        );
        let buf_cmd = buf_encoder.finish();

        // Encode egui render pass — egui-wgpu 0.29 requires RenderPass<'static>,
        // so we use forget_lifetime() to decouple from the encoder borrow.
        let mut rp_encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("egui_render"),
            });
        let rp = rp_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("egui_render_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        // SAFETY: forget_lifetime decouples the borrow from the encoder so
        // egui_renderer.render() can accept &mut RenderPass<'static>.
        // The render pass is dropped before encoder.finish() below.
        let mut rp_static = rp.forget_lifetime();
        egui_renderer.render(&mut rp_static, egui_primitives, screen_descriptor);
        drop(rp_static);
        let rp_cmd = rp_encoder.finish();

        self.queue.submit([video_cmd, buf_cmd, rp_cmd]);
        output.present();

        for id in &egui_textures_delta.free {
            egui_renderer.free_texture(id);
        }

        Ok(())
    }
}
