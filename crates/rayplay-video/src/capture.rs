use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CaptureError {
    #[error("screen capture is not supported on this platform")]
    UnsupportedPlatform,
    #[error("failed to initialize capture device: {0}")]
    InitializationFailed(String),
    #[error("failed to acquire frame: {0}")]
    AcquireFailed(String),
    #[error("capture timed out after {0:?}")]
    Timeout(Duration),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CaptureConfig {
    pub target_fps: u32,
    pub acquire_timeout_ms: u32,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            target_fps: 60,
            acquire_timeout_ms: 100,
        }
    }
}

#[derive(Debug)]
pub struct CapturedFrame {
    pub width: u32,
    pub height: u32,
    /// Row stride in bytes (may be larger than `width * 4` due to GPU alignment).
    pub stride: u32,
    /// Raw BGRA pixel data, `stride * height` bytes.
    pub data: Vec<u8>,
    pub timestamp: Instant,
}

impl CapturedFrame {
    /// Expected minimum buffer size in bytes.
    #[must_use]
    pub fn buffer_size(&self) -> usize {
        self.stride as usize * self.height as usize
    }
}

/// Builds a [`CapturedFrame`] from raw BGRA pixel data and dimensions.
///
/// Derives stride from the actual data length rather than assuming `width * 4`,
/// because scrap may return rows with platform-specific alignment padding.
pub(crate) fn build_frame(data: Vec<u8>, width: u32, height: u32) -> CapturedFrame {
    #[allow(clippy::cast_possible_truncation)]
    let stride = if height == 0 {
        width * 4
    } else {
        (data.len() / height as usize) as u32
    };
    CapturedFrame {
        width,
        height,
        stride,
        data,
        timestamp: Instant::now(),
    }
}

/// GPU-resident captured frame for the zero-copy encode path (Windows only).
///
/// Holds a raw pointer to an `ID3D11Texture2D` acquired from Desktop
/// Duplication.  The caller **must** call
/// [`ZeroCopyCapturer::release_frame`] after the encoder has consumed
/// the texture.
///
/// Timestamp is not a property of the captured texture itself — the caller
/// assigns a session-relative timestamp when constructing [`EncoderInput`].
#[cfg(target_os = "windows")]
pub struct CapturedTexture {
    /// Opaque pointer to `ID3D11Texture2D`.
    pub texture_ptr: *mut std::ffi::c_void,
    pub width: u32,
    pub height: u32,
}

// SAFETY: same single-thread model as DxgiCapture — the pointer is only
// dereferenced on the dedicated capture/encode thread.
#[cfg(target_os = "windows")]
unsafe impl Send for CapturedTexture {}

/// Zero-copy screen capturer that returns GPU-resident textures (Windows only).
///
/// Unlike [`ScreenCapturer`], the ownership contract requires the caller to
/// call [`release_frame`](ZeroCopyCapturer::release_frame) after encoding
/// completes.  This keeps the DXGI duplication frame locked only while the
/// encoder is reading it.
#[cfg(target_os = "windows")]
pub trait ZeroCopyCapturer: Send {
    /// Acquires the next desktop frame as a GPU texture.
    ///
    /// # Errors
    ///
    /// Returns [`CaptureError::AcquireFailed`] or [`CaptureError::Timeout`].
    fn acquire_texture(&self) -> Result<CapturedTexture, CaptureError>;

    /// Releases the most recently acquired frame back to the duplication API.
    fn release_frame(&self);

    /// Returns the capture resolution `(width, height)`.
    fn resolution(&self) -> (u32, u32);
}

pub trait ScreenCapturer: Send {
    /// Captures a single frame from the display.
    ///
    /// # Errors
    ///
    /// Returns [`CaptureError::AcquireFailed`] if the desktop duplication API
    /// fails to acquire a frame, or [`CaptureError::Timeout`] if no new frame
    /// is available within the configured timeout.
    fn capture_frame(&mut self) -> Result<CapturedFrame, CaptureError>;
    fn resolution(&self) -> (u32, u32);
}

#[cfg(test)]
mod tests;
