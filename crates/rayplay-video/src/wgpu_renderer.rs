//! GPU-accelerated frame renderer using `wgpu` (UC-005, ADR-005).
//!
//! [`WgpuRenderer`] uploads decoded frames as GPU textures and renders them
//! to a swap-chain surface via a full-screen triangle shader.  Both BGRA8
//! and NV12 pixel formats are supported with dedicated render pipelines.
//!
//! # Presentation path
//!
//! ```text
//! DecodedFrame → GPU texture upload → full-screen quad shader → swap chain
//! ```
//!
//! For hardware-decoded NV12 frames from `VideoToolbox`, the present mode is
//! `Mailbox` (drop stale frames) falling back to `Fifo` (vsync).

use crate::{
    DecodedFrame, PixelFormat,
    renderer::{RenderError, Renderer},
};

// ── WGSL shaders ──────────────────────────────────────────────────────────────

/// Full-screen triangle shader for BGRA8 frames.
///
/// The vertex shader generates a single triangle that covers the entire NDC
/// clip space in three vertices (no vertex buffer required).  UV coordinates
/// map (0, 0) = top-left to (1, 1) = bottom-right.
const BGRA_SHADER: &str = r"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32(vertex_index & 1u) * 4.0 - 1.0;
    let y = 1.0 - f32(vertex_index >> 1u) * 4.0;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(x * 0.5 + 0.5, 0.5 - y * 0.5);
    return out;
}

@group(0) @binding(0) var frame_texture: texture_2d<f32>;
@group(0) @binding(1) var frame_sampler: sampler;

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    return textureSample(frame_texture, frame_sampler, uv);
}
";

/// Full-screen triangle shader for NV12 frames with BT.709 YUV→RGB conversion.
const NV12_SHADER: &str = r"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32(vertex_index & 1u) * 4.0 - 1.0;
    let y = 1.0 - f32(vertex_index >> 1u) * 4.0;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(x * 0.5 + 0.5, 0.5 - y * 0.5);
    return out;
}

@group(0) @binding(0) var y_texture:  texture_2d<f32>;
@group(0) @binding(1) var uv_texture: texture_2d<f32>;
@group(0) @binding(2) var frame_sampler: sampler;

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let y_val  = textureSample(y_texture,  frame_sampler, uv).r;
    let uv_val = textureSample(uv_texture, frame_sampler, uv).rg;
    let u = uv_val.r - 0.5;
    let v = uv_val.g - 0.5;
    // BT.709 full-range YUV → RGB
    let r = y_val + 1.5748 * v;
    let g = y_val - 0.1873 * u - 0.4681 * v;
    let b = y_val + 1.8556 * u;
    return vec4<f32>(r, g, b, 1.0);
}
";

// ── Output target ──────────────────────────────────────────────────────────────

/// Where rendered frames are written.
///
/// `Surface` is used in production (window swap chain).  `Offscreen` is used
/// in tests and benchmarks where no window is available.
pub(crate) enum RendererOutput {
    Surface {
        surface: wgpu::Surface<'static>,
        config: wgpu::SurfaceConfiguration,
    },
    Offscreen {
        texture: wgpu::Texture,
    },
}

// ── Texture cache ──────────────────────────────────────────────────────────────

/// Per-format GPU texture cache.
///
/// Cached textures are reused across frames when dimensions stay the same,
/// avoiding per-frame allocation on the GPU.
enum TextureCache {
    Bgra {
        texture: wgpu::Texture,
        bind_group: wgpu::BindGroup,
        width: u32,
        height: u32,
    },
    Nv12 {
        y_texture: wgpu::Texture,
        uv_texture: wgpu::Texture,
        bind_group: wgpu::BindGroup,
        width: u32,
        height: u32,
    },
}

// ── WgpuRenderer ──────────────────────────────────────────────────────────────

/// GPU-accelerated frame renderer backed by `wgpu` + Metal (macOS).
///
/// Created by [`RenderWindow::run`] after the `winit` window is available.
/// Implements [`Renderer`] so it can be swapped for a stub in tests.
pub struct WgpuRenderer {
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pub(crate) output: RendererOutput,
    bgra_pipeline: wgpu::RenderPipeline,
    nv12_pipeline: wgpu::RenderPipeline,
    bgra_bgl: wgpu::BindGroupLayout,
    pub(crate) nv12_bgl: wgpu::BindGroupLayout,
    pub(crate) sampler: wgpu::Sampler,
    texture_cache: Option<TextureCache>,
}

impl WgpuRenderer {
    /// Creates a renderer that renders to an off-screen texture.
    ///
    /// Used for testing and benchmarking without a window.  Rendered output
    /// is written to an internal `Rgba8Unorm` texture; call
    /// [`present_frame`](Self::present_frame) normally.
    #[must_use]
    pub fn new_offscreen(
        device: wgpu::Device,
        queue: wgpu::Queue,
        width: u32,
        height: u32,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen_output"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let output = RendererOutput::Offscreen { texture };
        Self::from_parts(device, queue, output, wgpu::TextureFormat::Rgba8Unorm)
    }

    /// Shared initialisation path for both surface and offscreen renderers.
    ///
    /// `surface_format` must be passed explicitly — for offscreen use
    /// [`wgpu::TextureFormat::Rgba8Unorm`]; for surface use `config.format`.
    pub(crate) fn from_parts(
        device: wgpu::Device,
        queue: wgpu::Queue,
        output: RendererOutput,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("frame_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let (bgra_bgl, bgra_pipeline) = build_bgra_pipeline(&device, surface_format, BGRA_SHADER);
        let (nv12_bgl, nv12_pipeline) = build_nv12_pipeline(&device, surface_format, NV12_SHADER);
        Self {
            device,
            queue,
            output,
            bgra_pipeline,
            nv12_pipeline,
            bgra_bgl,
            nv12_bgl,
            sampler,
            texture_cache: None,
        }
    }

    /// Reconfigures the swap chain after a window resize.
    ///
    /// Call this when [`RenderError::SurfaceLost`] is returned from
    /// [`present_frame`](Self::present_frame), or whenever the window emits a
    /// `Resized` event.  No-op when using the offscreen output target.
    ///
    /// Surface-specific implementation lives in `wgpu_surface.rs` (excluded from
    /// the unit-test coverage gate, as it requires a live swap chain).
    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.apply_resize(new_size);
    }

    // ── Internal helpers ───────────────────────────────────────────────────────

    fn texture_matches(&self, frame: &DecodedFrame) -> bool {
        match (&self.texture_cache, &frame.format) {
            (Some(TextureCache::Bgra { width, height, .. }), PixelFormat::Bgra8)
            | (Some(TextureCache::Nv12 { width, height, .. }), PixelFormat::Nv12) => {
                *width == frame.width && *height == frame.height
            }
            _ => false,
        }
    }

    fn create_bgra_cache(&self, width: u32, height: u32) -> TextureCache {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("bgra_frame_texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bgra_bind_group"),
            layout: &self.bgra_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        TextureCache::Bgra {
            texture,
            bind_group,
            width,
            height,
        }
    }

    fn create_nv12_cache(&self, width: u32, height: u32) -> TextureCache {
        let y_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("nv12_y_texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let uv_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("nv12_uv_texture"),
            size: wgpu::Extent3d {
                width: width / 2,
                height: height / 2,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rg8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let y_view = y_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let uv_view = uv_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("nv12_bind_group"),
            layout: &self.nv12_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&y_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&uv_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        TextureCache::Nv12 {
            y_texture,
            uv_texture,
            bind_group,
            width,
            height,
        }
    }

    fn upload_bgra(&self, frame: &DecodedFrame, texture: &wgpu::Texture) {
        self.queue.write_texture(
            texture.as_image_copy(),
            &frame.data,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(frame.stride),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: frame.width,
                height: frame.height,
                depth_or_array_layers: 1,
            },
        );
    }

    fn upload_nv12(
        &self,
        frame: &DecodedFrame,
        y_texture: &wgpu::Texture,
        uv_texture: &wgpu::Texture,
    ) {
        let y_end = frame.stride as usize * frame.height as usize;
        self.queue.write_texture(
            y_texture.as_image_copy(),
            &frame.data[..y_end],
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(frame.stride),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: frame.width,
                height: frame.height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.write_texture(
            uv_texture.as_image_copy(),
            &frame.data[y_end..],
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(frame.stride),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: frame.width / 2,
                height: frame.height / 2,
                depth_or_array_layers: 1,
            },
        );
    }

    fn upload_frame(&self, frame: &DecodedFrame) {
        match (&self.texture_cache, &frame.format) {
            (Some(TextureCache::Bgra { texture, .. }), PixelFormat::Bgra8) => {
                self.upload_bgra(frame, texture);
            }
            (
                Some(TextureCache::Nv12 {
                    y_texture,
                    uv_texture,
                    ..
                }),
                PixelFormat::Nv12,
            ) => {
                self.upload_nv12(frame, y_texture, uv_texture);
            }
            _ => {}
        }
    }

    /// Encodes a full-screen render pass into a command buffer.
    ///
    /// When `override_nv12_bind_group` is `Some`, it is used in place of the
    /// cached NV12 bind group (`IOSurface` zero-copy path).
    pub(crate) fn encode_frame(
        &self,
        output_view: &wgpu::TextureView,
        override_nv12_bind_group: Option<&wgpu::BindGroup>,
    ) -> wgpu::CommandBuffer {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render_encoder"),
            });
        {
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("render_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            let has_pipeline = if let Some(bg) = override_nv12_bind_group {
                rp.set_pipeline(&self.nv12_pipeline);
                rp.set_bind_group(0, bg, &[]);
                true
            } else {
                match self.texture_cache.as_ref() {
                    Some(TextureCache::Bgra { bind_group, .. }) => {
                        rp.set_pipeline(&self.bgra_pipeline);
                        rp.set_bind_group(0, bind_group, &[]);
                        true
                    }
                    Some(TextureCache::Nv12 { bind_group, .. }) => {
                        rp.set_pipeline(&self.nv12_pipeline);
                        rp.set_bind_group(0, bind_group, &[]);
                        true
                    }
                    None => false,
                }
            };
            if has_pipeline {
                rp.draw(0..3, 0..1);
            }
        }
        encoder.finish()
    }
}

impl Renderer for WgpuRenderer {
    fn present_frame(&mut self, frame: &DecodedFrame) -> Result<(), RenderError> {
        // ── IOSurface zero-copy path (macOS hardware frames) ────────────
        #[cfg(target_os = "macos")]
        let hw_bind_group = if frame.is_hardware_frame {
            if let Some(ref handle) = frame.iosurface {
                let bg = self.import_iosurface_textures(handle, frame.width, frame.height);
                if bg.is_none() {
                    tracing::warn!("IOSurface import failed; falling back to clear-only render");
                }
                bg
            } else {
                tracing::warn!(
                    "hardware frame missing IOSurface handle; falling back to clear-only render"
                );
                None
            }
        } else {
            None
        };

        #[cfg(not(target_os = "macos"))]
        let hw_bind_group: Option<wgpu::BindGroup> = None;

        // ── CPU upload path (software frames) ───────────────────────────
        if hw_bind_group.is_none() && !frame.is_hardware_frame {
            if !self.texture_matches(frame) {
                self.texture_cache = Some(match frame.format {
                    PixelFormat::Bgra8 => self.create_bgra_cache(frame.width, frame.height),
                    PixelFormat::Nv12 => self.create_nv12_cache(frame.width, frame.height),
                });
            }
            self.upload_frame(frame);
        }

        // ── Render ──────────────────────────────────────────────────────
        // Offscreen path: render to the internal texture (used in tests/benchmarks).
        if let RendererOutput::Offscreen { texture, .. } = &self.output {
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let cmd = self.encode_frame(&view, hw_bind_group.as_ref());
            self.queue.submit(std::iter::once(cmd));
            return Ok(());
        }
        // Surface path: acquire swap-chain frame, render, present.
        // Implementation lives in wgpu_surface.rs (excluded from the unit-test
        // coverage gate, as it requires a live swap chain).
        self.present_to_surface(hw_bind_group.as_ref())
    }
}

// ── Free helper functions (unit-testable without GPU) ─────────────────────────

/// Selects `Bgra8Unorm` from the adapter's supported surface formats if
/// available, falling back to the first supported format.
pub(crate) fn select_surface_format(formats: &[wgpu::TextureFormat]) -> wgpu::TextureFormat {
    formats
        .iter()
        .find(|&&f| f == wgpu::TextureFormat::Bgra8Unorm)
        .copied()
        .unwrap_or_else(|| {
            formats
                .first()
                .copied()
                .unwrap_or(wgpu::TextureFormat::Bgra8Unorm)
        })
}

/// Prefers `Mailbox` present mode (drops stale frames) for streaming freshness.
/// Falls back to `Fifo` (vsync) when `Mailbox` is unavailable.
pub(crate) fn select_present_mode(modes: &[wgpu::PresentMode]) -> wgpu::PresentMode {
    if modes.contains(&wgpu::PresentMode::Mailbox) {
        wgpu::PresentMode::Mailbox
    } else {
        wgpu::PresentMode::Fifo
    }
}

/// Maps a `wgpu::SurfaceError` to a [`RenderError`].
pub(crate) fn surface_error_to_render_error(e: &wgpu::SurfaceError) -> RenderError {
    match e {
        wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated => RenderError::SurfaceLost,
        wgpu::SurfaceError::OutOfMemory => RenderError::Failed {
            reason: "GPU out of memory".to_string(),
        },
        wgpu::SurfaceError::Timeout => RenderError::Failed {
            reason: "surface acquire timeout".to_string(),
        },
    }
}

/// Builds the BGRA render pipeline and its bind group layout.
fn build_bgra_pipeline(
    device: &wgpu::Device,
    surface_format: wgpu::TextureFormat,
    wgsl: &str,
) -> (wgpu::BindGroupLayout, wgpu::RenderPipeline) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("bgra_shader"),
        source: wgpu::ShaderSource::Wgsl(wgsl.into()),
    });
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("bgra_bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("bgra_pipeline_layout"),
        bind_group_layouts: &[&bgl],
        push_constant_ranges: &[],
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("bgra_pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs_main",
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs_main",
            targets: &[Some(wgpu::ColorTargetState {
                format: surface_format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    });
    (bgl, pipeline)
}

/// Builds the NV12 render pipeline and its bind group layout.
fn build_nv12_pipeline(
    device: &wgpu::Device,
    surface_format: wgpu::TextureFormat,
    wgsl: &str,
) -> (wgpu::BindGroupLayout, wgpu::RenderPipeline) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("nv12_shader"),
        source: wgpu::ShaderSource::Wgsl(wgsl.into()),
    });
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("nv12_bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("nv12_pipeline_layout"),
        bind_group_layouts: &[&bgl],
        push_constant_ranges: &[],
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("nv12_pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs_main",
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs_main",
            targets: &[Some(wgpu::ColorTargetState {
                format: surface_format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    });
    (bgl, pipeline)
}

#[cfg(test)]
mod tests;
