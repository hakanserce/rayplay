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

#[cfg(target_os = "macos")]
use crate::decoded_frame::IoSurfaceHandle;

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
    nv12_bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
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

    /// Imports an `IOSurface` as NV12 Metal textures and creates a bind group.
    ///
    /// Returns `None` if any step fails (logged as a warning).  The caller
    /// should fall back to a clear-only render pass.
    #[cfg(target_os = "macos")]
    #[allow(clippy::too_many_lines, unexpected_cfgs)]
    fn import_iosurface_textures(
        &self,
        handle: &IoSurfaceHandle,
        width: u32,
        height: u32,
    ) -> Option<wgpu::BindGroup> {
        use metal::foreign_types::ForeignType;
        use objc::msg_send;
        use objc::sel;
        use objc::sel_impl;

        let iosurface_ptr = handle.as_ptr();

        // SAFETY: wgpu is running on the Metal backend on macOS.
        unsafe {
            self.device
                .as_hal::<wgpu::hal::metal::Api, _, Option<wgpu::BindGroup>>(|hal_device| {
                    let hal_device = hal_device?;
                    let raw_device = hal_device.raw_device().lock();

                    // ── Y plane (plane 0): R8Unorm ──────────────────────
                    let y_desc = metal::TextureDescriptor::new();
                    y_desc.set_texture_type(metal::MTLTextureType::D2);
                    y_desc.set_pixel_format(metal::MTLPixelFormat::R8Unorm);
                    y_desc.set_width(u64::from(width));
                    y_desc.set_height(u64::from(height));
                    y_desc.set_storage_mode(metal::MTLStorageMode::Shared);
                    y_desc.set_usage(metal::MTLTextureUsage::ShaderRead);

                    let y_raw: *mut metal::MTLTexture = msg_send![
                        raw_device.as_ref(),
                        newTextureWithDescriptor:y_desc.as_ref()
                        iosurface:iosurface_ptr
                        plane:0usize
                    ];
                    if y_raw.is_null() {
                        tracing::warn!("Metal newTextureWithDescriptor failed for Y plane");
                        return None;
                    }
                    let y_metal = metal::Texture::from_ptr(y_raw.cast());

                    // ── UV plane (plane 1): Rg8Unorm ────────────────────
                    let uv_desc = metal::TextureDescriptor::new();
                    uv_desc.set_texture_type(metal::MTLTextureType::D2);
                    uv_desc.set_pixel_format(metal::MTLPixelFormat::RG8Unorm);
                    uv_desc.set_width(u64::from(width / 2));
                    uv_desc.set_height(u64::from(height / 2));
                    uv_desc.set_storage_mode(metal::MTLStorageMode::Shared);
                    uv_desc.set_usage(metal::MTLTextureUsage::ShaderRead);

                    let uv_raw: *mut metal::MTLTexture = msg_send![
                        raw_device.as_ref(),
                        newTextureWithDescriptor:uv_desc.as_ref()
                        iosurface:iosurface_ptr
                        plane:1usize
                    ];
                    if uv_raw.is_null() {
                        tracing::warn!("Metal newTextureWithDescriptor failed for UV plane");
                        return None;
                    }
                    let uv_metal = metal::Texture::from_ptr(uv_raw.cast());

                    // ── Import Y texture into wgpu ──────────────────────
                    let y_hal = wgpu::hal::metal::Device::texture_from_raw(
                        y_metal,
                        wgpu::TextureFormat::R8Unorm,
                        metal::MTLTextureType::D2,
                        1,
                        1,
                        wgpu::hal::CopyExtent {
                            width,
                            height,
                            depth: 1,
                        },
                    );
                    let y_wgpu = self
                        .device
                        .create_texture_from_hal::<wgpu::hal::metal::Api>(
                            y_hal,
                            &wgpu::TextureDescriptor {
                                label: Some("iosurface_y"),
                                size: wgpu::Extent3d {
                                    width,
                                    height,
                                    depth_or_array_layers: 1,
                                },
                                mip_level_count: 1,
                                sample_count: 1,
                                dimension: wgpu::TextureDimension::D2,
                                format: wgpu::TextureFormat::R8Unorm,
                                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                                view_formats: &[],
                            },
                        );

                    // ── Import UV texture into wgpu ─────────────────────
                    let uv_hal = wgpu::hal::metal::Device::texture_from_raw(
                        uv_metal,
                        wgpu::TextureFormat::Rg8Unorm,
                        metal::MTLTextureType::D2,
                        1,
                        1,
                        wgpu::hal::CopyExtent {
                            width: width / 2,
                            height: height / 2,
                            depth: 1,
                        },
                    );
                    let uv_wgpu = self
                        .device
                        .create_texture_from_hal::<wgpu::hal::metal::Api>(
                            uv_hal,
                            &wgpu::TextureDescriptor {
                                label: Some("iosurface_uv"),
                                size: wgpu::Extent3d {
                                    width: width / 2,
                                    height: height / 2,
                                    depth_or_array_layers: 1,
                                },
                                mip_level_count: 1,
                                sample_count: 1,
                                dimension: wgpu::TextureDimension::D2,
                                format: wgpu::TextureFormat::Rg8Unorm,
                                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                                view_formats: &[],
                            },
                        );

                    // ── Build bind group ─────────────────────────────────
                    let y_view = y_wgpu.create_view(&wgpu::TextureViewDescriptor::default());
                    let uv_view = uv_wgpu.create_view(&wgpu::TextureViewDescriptor::default());

                    Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("iosurface_nv12_bind_group"),
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
                    }))
                })
                .flatten()
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

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
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

    // ── IOSurface import (macOS only) ─────────────────────────────────────────

    /// Creates an NV12 `CVPixelBuffer` with forced `IOSurface` backing and
    /// returns the retained `IOSurfaceRef`, or null on failure.
    ///
    /// `kCVPixelBufferIOSurfacePropertiesKey` in the attributes dict forces
    /// the system to allocate an `IOSurface`-backed biplanar pixel buffer.
    #[cfg(target_os = "macos")]
    unsafe fn create_nv12_iosurface(width: u32, height: u32) -> *mut std::ffi::c_void {
        use std::ffi::c_void;

        #[link(name = "CoreVideo", kind = "framework")]
        unsafe extern "C" {
            static kCVPixelBufferIOSurfacePropertiesKey: *const c_void; // CFStringRef
            fn CVPixelBufferCreate(
                allocator: *const c_void,
                width: usize,
                height: usize,
                pixel_format_type: u32,
                pixel_buffer_attributes: *const c_void,
                pixel_buffer_out: *mut *mut c_void,
            ) -> i32;
            fn CVPixelBufferGetIOSurface(pixel_buffer: *mut c_void) -> *mut c_void;
        }
        #[link(name = "CoreFoundation", kind = "framework")]
        unsafe extern "C" {
            fn CFDictionaryCreate(
                alloc: *const c_void,
                keys: *const *const c_void,
                values: *const *const c_void,
                num_values: isize,
                key_callbacks: *const c_void,
                value_callbacks: *const c_void,
            ) -> *mut c_void;
            fn CFRetain(cf: *const c_void) -> *const c_void;
            fn CFRelease(cf: *const c_void);
            static kCFTypeDictionaryKeyCallBacks: c_void;
            static kCFTypeDictionaryValueCallBacks: c_void;
        }

        // Empty dict → value for kCVPixelBufferIOSurfacePropertiesKey.
        let iosurface_props = unsafe {
            CFDictionaryCreate(
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                &raw const kCFTypeDictionaryKeyCallBacks as *const c_void,
                &raw const kCFTypeDictionaryValueCallBacks as *const c_void,
            )
        };
        // Attributes dict: { IOSurfaceProperties → {} }
        let attr_key: *const c_void = unsafe { kCVPixelBufferIOSurfacePropertiesKey };
        let attr_val: *const c_void = iosurface_props.cast_const();
        let attrs = unsafe {
            CFDictionaryCreate(
                std::ptr::null(),
                &raw const attr_key as *const *const c_void,
                &raw const attr_val as *const *const c_void,
                1,
                &raw const kCFTypeDictionaryKeyCallBacks as *const c_void,
                &raw const kCFTypeDictionaryValueCallBacks as *const c_void,
            )
        };
        // kCVPixelFormatType_420YpCbCr8BiPlanarFullRange = 875704438
        let pixel_format: u32 = 875_704_438;
        let mut pixel_buffer: *mut c_void = std::ptr::null_mut();
        let status = unsafe {
            CVPixelBufferCreate(
                std::ptr::null(),
                width as usize,
                height as usize,
                pixel_format,
                attrs,
                &raw mut pixel_buffer,
            )
        };
        unsafe { CFRelease(attrs.cast_const()) };
        unsafe { CFRelease(iosurface_props.cast_const()) };
        if status != 0 || pixel_buffer.is_null() {
            return std::ptr::null_mut();
        }
        let surface = unsafe { CVPixelBufferGetIOSurface(pixel_buffer) };
        if !surface.is_null() {
            unsafe { CFRetain(surface.cast_const()) };
        }
        unsafe { CFRelease(pixel_buffer.cast_const()) };
        surface
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_create_nv12_iosurface_fails_on_zero_dimensions() {
        // CVPixelBufferCreate rejects zero-dimension buffers; must return null.
        let surface = unsafe { create_nv12_iosurface(0, 0) };
        assert!(
            surface.is_null(),
            "expected null for zero-dimension IOSurface"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_import_iosurface_textures_with_real_surface() {
        use crate::decoded_frame::IoSurfaceHandle;

        let r = make_offscreen(64, 64);
        let surface = unsafe { create_nv12_iosurface(64, 64) };
        // IOSurface creation must succeed on Apple hardware.
        assert!(!surface.is_null(), "create_nv12_iosurface should succeed");
        // SAFETY: surface is a retained IOSurfaceRef.
        let handle = unsafe { IoSurfaceHandle::from_retained(surface) };
        // The import may succeed or return None; either path must not panic.
        let _result = r.import_iosurface_textures(&handle, 64, 64);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_present_hardware_frame_with_real_iosurface() {
        use crate::decoded_frame::IoSurfaceHandle;

        let surface = unsafe { create_nv12_iosurface(64, 64) };
        // IOSurface creation must succeed on Apple hardware.
        assert!(!surface.is_null(), "create_nv12_iosurface should succeed");
        // SAFETY: surface is a retained IOSurfaceRef.
        let handle = unsafe { IoSurfaceHandle::from_retained(surface) };
        let mut r = make_offscreen(64, 64);
        let frame = DecodedFrame::new_hardware(64, 64, 64, PixelFormat::Nv12, 0, handle);
        assert!(r.present_frame(&frame).is_ok());
    }
}
