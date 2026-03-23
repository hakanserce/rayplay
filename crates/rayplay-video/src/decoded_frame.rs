// ── IoSurfaceHandle — platform-specific implementation ───────────────────────

/// RAII wrapper for a retained `IOSurfaceRef` (macOS).
///
/// On macOS, calls `CFRetain` on creation and `CFRelease` on drop.
/// On other platforms, this is an uninhabited stub — `Option<IoSurfaceHandle>`
/// is always `None` and the type compiles away to zero cost.
#[derive(Debug)]
#[cfg(target_os = "macos")]
pub struct IoSurfaceHandle {
    ptr: *mut std::ffi::c_void,
}

#[cfg(target_os = "macos")]
mod iosurface_impl {
    use std::ffi::c_void;

    unsafe extern "C" {
        fn CFRetain(cf: *const c_void) -> *const c_void;
        fn CFRelease(cf: *const c_void);
    }

    use super::IoSurfaceHandle;

    impl IoSurfaceHandle {
        /// Wraps an already-retained `IOSurfaceRef`.
        ///
        /// # Safety
        ///
        /// `ptr` must be a valid, retained `IOSurfaceRef`.
        #[must_use]
        pub unsafe fn from_retained(ptr: *mut c_void) -> Self {
            Self { ptr }
        }

        /// Returns the raw `IOSurfaceRef` pointer.
        #[must_use]
        pub fn as_ptr(&self) -> *mut c_void {
            self.ptr
        }
    }

    impl Clone for IoSurfaceHandle {
        fn clone(&self) -> Self {
            unsafe { CFRetain(self.ptr.cast_const()) };
            Self { ptr: self.ptr }
        }
    }

    impl Drop for IoSurfaceHandle {
        fn drop(&mut self) {
            unsafe { CFRelease(self.ptr.cast_const()) };
        }
    }

    // SAFETY: IOSurface is a kernel-managed shared-memory object, safe from any thread.
    unsafe impl Send for IoSurfaceHandle {}
    unsafe impl Sync for IoSurfaceHandle {}
}

/// Stub `IoSurfaceHandle` for non-macOS platforms.
///
/// This type exists so `DecodedFrame` can have an unconditional
/// `iosurface: Option<IoSurfaceHandle>` field. The type is uninhabited
/// (has no constructors), so `Option<IoSurfaceHandle>` is always `None`
/// and optimises to zero size.
#[cfg(not(target_os = "macos"))]
#[derive(Debug, Clone)]
pub enum IoSurfaceHandle {}

/// Pixel format of a decoded video frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PixelFormat {
    /// BGRA 8-bit interleaved — CPU-side representation, GPU-uploadable directly.
    Bgra8,
    /// YUV 4:2:0 biplanar (NV12) — native `VideoToolbox` output format on macOS.
    ///
    /// Plane 0 is luma (Y), plane 1 is interleaved chroma (UV).
    Nv12,
}

impl PixelFormat {
    /// Returns the bytes per pixel for packed formats, or `None` for planar formats.
    #[must_use]
    pub fn bytes_per_pixel(&self) -> Option<usize> {
        match self {
            Self::Bgra8 => Some(4),
            Self::Nv12 => None,
        }
    }
}

/// A decoded video frame ready for rendering.
///
/// On macOS, hardware-decoded frames are backed by a `CVPixelBuffer` (`IOSurface`).
/// When `is_hardware_frame` is `true`, `data` is empty and the frame lives in
/// GPU-resident memory; the renderer imports it via `IOSurface` interop (ADR-005)
/// rather than uploading via `write_texture`.
///
/// The `iosurface` field is always present in the struct definition. On non-macOS
/// platforms `IoSurfaceHandle` is uninhabited, so `Option<IoSurfaceHandle>` is
/// always `None` and compiles to zero size.
#[derive(Debug, Clone)]
pub struct DecodedFrame {
    /// Decoded pixel data. Empty when `is_hardware_frame` is `true`.
    pub data: Vec<u8>,
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Row stride in bytes (first-plane stride for NV12; interleaved stride for BGRA).
    pub stride: u32,
    /// Pixel format of the decoded data.
    pub format: PixelFormat,
    /// Presentation timestamp in microseconds.
    pub timestamp_us: u64,
    /// Whether the frame is backed by GPU-resident `IOSurface` memory (macOS).
    pub is_hardware_frame: bool,
    /// Retained `IOSurface` handle for zero-copy GPU rendering (macOS only).
    /// Always `None` on non-macOS platforms.
    pub iosurface: Option<IoSurfaceHandle>,
}

impl DecodedFrame {
    /// Creates a CPU-side decoded frame (software decode or mock).
    #[must_use]
    pub fn new_cpu(
        data: Vec<u8>,
        width: u32,
        height: u32,
        stride: u32,
        format: PixelFormat,
        timestamp_us: u64,
    ) -> Self {
        Self {
            data,
            width,
            height,
            stride,
            format,
            timestamp_us,
            is_hardware_frame: false,
            iosurface: None,
        }
    }

    /// Creates a hardware-backed frame with an `IOSurface` handle (macOS).
    ///
    /// `data` is left empty; the frame data lives in GPU memory and the
    /// renderer imports it via the `iosurface` handle.
    #[cfg(target_os = "macos")]
    #[must_use]
    pub fn new_hardware(
        width: u32,
        height: u32,
        stride: u32,
        format: PixelFormat,
        timestamp_us: u64,
        iosurface: IoSurfaceHandle,
    ) -> Self {
        Self {
            data: Vec::new(),
            width,
            height,
            stride,
            format,
            timestamp_us,
            is_hardware_frame: true,
            iosurface: Some(iosurface),
        }
    }

    /// Creates a hardware-backed frame stub for testing.
    ///
    /// Sets `iosurface` to `None`. Used by unit tests that do not have a real
    /// `IOSurface`. The renderer falls back to a clear-only render pass.
    #[cfg(test)]
    #[must_use]
    pub fn new_hardware_test_stub(
        width: u32,
        height: u32,
        stride: u32,
        format: PixelFormat,
        timestamp_us: u64,
    ) -> Self {
        Self {
            data: Vec::new(),
            width,
            height,
            stride,
            format,
            timestamp_us,
            is_hardware_frame: true,
            iosurface: None,
        }
    }

    /// Expected byte size of `data` for CPU frames.
    ///
    /// - `Bgra8`: `stride × height`
    /// - `Nv12`: `stride × height × 3 / 2` (luma plane + half-height chroma plane)
    /// - Hardware frames: `0` (data is GPU-resident)
    #[must_use]
    pub fn expected_data_size(&self) -> usize {
        if self.is_hardware_frame {
            return 0;
        }
        let h = self.height as usize;
        let s = self.stride as usize;
        match self.format {
            PixelFormat::Bgra8 => s * h,
            PixelFormat::Nv12 => s * h * 3 / 2,
        }
    }
}

#[cfg(test)]
mod tests;
