#[cfg(target_os = "macos")]
use std::ffi::c_void;

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn CFRetain(cf: *const c_void) -> *const c_void;
    fn CFRelease(cf: *const c_void);
}

/// RAII wrapper for a retained `IOSurfaceRef` (macOS).
///
/// Calls `CFRetain` on creation and `CFRelease` on drop. Cloning retains again.
/// The inner pointer is `Send + Sync` — `IOSurface` is a kernel-managed
/// shared-memory object safe to reference from any thread.
#[cfg(target_os = "macos")]
#[derive(Debug)]
pub struct IoSurfaceHandle {
    ptr: *mut c_void,
}

#[cfg(target_os = "macos")]
impl IoSurfaceHandle {
    /// Wraps an already-retained `IOSurfaceRef`. The caller must have called
    /// `CFRetain` (or equivalent) before handing the pointer here.
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

#[cfg(target_os = "macos")]
impl Clone for IoSurfaceHandle {
    fn clone(&self) -> Self {
        // SAFETY: ptr is a valid IOSurfaceRef; CFRetain returns the same pointer.
        unsafe { CFRetain(self.ptr.cast_const()) };
        Self { ptr: self.ptr }
    }
}

#[cfg(target_os = "macos")]
impl Drop for IoSurfaceHandle {
    fn drop(&mut self) {
        // SAFETY: ptr was retained on creation (and on each clone).
        unsafe { CFRelease(self.ptr.cast_const()) };
    }
}

// SAFETY: IOSurface is a kernel-managed shared-memory object. The underlying
// surface is reference-counted and safe to access from any thread.
#[cfg(target_os = "macos")]
unsafe impl Send for IoSurfaceHandle {}
#[cfg(target_os = "macos")]
unsafe impl Sync for IoSurfaceHandle {}

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
    ///
    /// `Nv12` returns `None` because its byte count depends on frame dimensions;
    /// use `DecodedFrame::expected_data_size` instead.
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
/// # Field Visibility
///
/// All fields are `pub` for direct access by the renderer (UC-005). The two
/// constructors (`new_cpu`, `new_hardware`) are the canonical way to create a
/// frame and enforce the `is_hardware_frame ↔ data.is_empty()` invariant, but
/// the invariant is not enforced at the type level — callers must not mutate
/// fields directly in a way that breaks it.
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
    #[cfg(target_os = "macos")]
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
            #[cfg(target_os = "macos")]
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

    /// Creates a hardware-backed frame stub for testing (macOS only).
    ///
    /// Sets `iosurface` to `None`. Used by unit tests that do not have a real
    /// `IOSurface`. The renderer falls back to a clear-only render pass.
    #[cfg(all(target_os = "macos", test))]
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
