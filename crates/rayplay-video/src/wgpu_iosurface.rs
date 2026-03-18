//! `IOSurface` → Metal → wgpu texture import for zero-copy NV12 rendering
//! (UC-005, ADR-005).
//!
//! # Coverage exclusion
//!
//! This file is excluded from the workspace line-coverage gate via
//! `--ignore-filename-regex` in `coverage-ci` (see `Makefile.toml`).
//! Every function uses macOS Metal FFI (`objc::msg_send!`) to import an
//! `IOSurface` as a pair of Metal textures and then wrap them in wgpu
//! bind groups.  These calls require a live GPU context backed by the
//! Metal driver — a resource that cannot be fabricated on headless CI.
//!
//! All other GPU code lives in `wgpu_renderer.rs` and is tested via the
//! headless offscreen path.

use crate::decoded_frame::IoSurfaceHandle;
use crate::wgpu_renderer::WgpuRenderer;

impl WgpuRenderer {
    /// Imports an `IOSurface` as NV12 Metal textures and creates a bind group.
    ///
    /// Returns `None` if any step fails (logged as a warning).  The caller
    /// should fall back to a clear-only render pass.
    #[allow(clippy::too_many_lines, unexpected_cfgs)]
    pub(crate) fn import_iosurface_textures(
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
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use crate::wgpu_renderer::WgpuRenderer;
    use crate::{DecodedFrame, PixelFormat};

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

    /// Creates an NV12 `CVPixelBuffer` with forced `IOSurface` backing and
    /// returns the retained `IOSurfaceRef`, or null on failure.
    ///
    /// `kCVPixelBufferIOSurfacePropertiesKey` in the attributes dict forces
    /// the system to allocate an `IOSurface`-backed biplanar pixel buffer.
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

    #[test]
    fn test_create_nv12_iosurface_fails_on_zero_dimensions() {
        // CVPixelBufferCreate rejects zero-dimension buffers; must return null.
        let surface = unsafe { create_nv12_iosurface(0, 0) };
        assert!(
            surface.is_null(),
            "expected null for zero-dimension IOSurface"
        );
    }

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

    #[test]
    fn test_present_hardware_frame_with_real_iosurface() {
        use crate::decoded_frame::IoSurfaceHandle;
        use crate::renderer::Renderer;

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
