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

/// Returns the platform-appropriate screen capturer.
///
/// On Windows, returns a [`DxgiCapture`](crate::dxgi_capture::DxgiCapture) backed by
/// DXGI Desktop Duplication.  On other platforms returns
/// [`CaptureError::UnsupportedPlatform`].
///
/// # Errors
///
/// Returns [`CaptureError::InitializationFailed`] if the D3D11 device or output
/// duplication cannot be created.
pub fn create_capturer(config: CaptureConfig) -> Result<Box<dyn ScreenCapturer>, CaptureError> {
    #[cfg(target_os = "windows")]
    {
        use std::sync::Arc;

        use crate::d3d11_device::SharedD3D11Device;
        use crate::dxgi_capture::DxgiCapture;

        let device = Arc::new(SharedD3D11Device::new()?);
        DxgiCapture::new(config, device).map(|c| Box::new(c) as Box<dyn ScreenCapturer>)
    }
    #[cfg(not(target_os = "windows"))]
    {
        #[cfg(feature = "fallback")]
        {
            use crate::scrap_capture::ScrapCapturer;
            ScrapCapturer::new(config).map(|c| Box::new(c) as Box<dyn ScreenCapturer>)
        }
        #[cfg(not(feature = "fallback"))]
        {
            let _ = config;
            Err(CaptureError::UnsupportedPlatform)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── CaptureConfig ──────────────────────────────────────────────────────────

    #[test]
    fn test_capture_config_default_fps() {
        assert_eq!(CaptureConfig::default().target_fps, 60);
    }

    #[test]
    fn test_capture_config_default_timeout() {
        assert_eq!(CaptureConfig::default().acquire_timeout_ms, 100);
    }

    #[test]
    fn test_capture_config_clone() {
        let cfg = CaptureConfig {
            target_fps: 30,
            acquire_timeout_ms: 50,
        };
        let cloned = cfg.clone();
        assert_eq!(cloned.target_fps, 30);
        assert_eq!(cloned.acquire_timeout_ms, 50);
    }

    #[test]
    fn test_capture_config_serde_roundtrip() {
        let cfg = CaptureConfig {
            target_fps: 120,
            acquire_timeout_ms: 200,
        };
        let json = serde_json::to_string(&cfg).expect("serialize");
        let back: CaptureConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.target_fps, 120);
        assert_eq!(back.acquire_timeout_ms, 200);
    }

    // ── CapturedFrame ──────────────────────────────────────────────────────────

    #[test]
    fn test_captured_frame_buffer_size() {
        let frame = CapturedFrame {
            width: 1920,
            height: 1080,
            stride: 7680, // 1920 * 4
            data: vec![0u8; 7680 * 1080],
            timestamp: Instant::now(),
        };
        assert_eq!(frame.buffer_size(), 7680 * 1080);
    }

    #[test]
    fn test_captured_frame_buffer_size_with_padding() {
        // Stride may include alignment padding (e.g. 7936 instead of 7680).
        let stride = 7936_u32;
        let height = 1080_u32;
        let frame = CapturedFrame {
            width: 1920,
            height,
            stride,
            data: vec![0u8; (stride * height) as usize],
            timestamp: Instant::now(),
        };
        assert_eq!(frame.buffer_size(), (stride * height) as usize);
    }

    // ── CaptureError ──────────────────────────────────────────────────────────

    #[test]
    fn test_capture_error_unsupported_platform_display() {
        let msg = CaptureError::UnsupportedPlatform.to_string();
        assert!(msg.contains("not supported"));
    }

    #[test]
    fn test_capture_error_initialization_failed_display() {
        let msg = CaptureError::InitializationFailed("no adapter".into()).to_string();
        assert!(msg.contains("initialize"));
        assert!(msg.contains("no adapter"));
    }

    #[test]
    fn test_capture_error_acquire_failed_display() {
        let msg = CaptureError::AcquireFailed("DXGI_ERROR_ACCESS_LOST".into()).to_string();
        assert!(msg.contains("acquire"));
        assert!(msg.contains("DXGI_ERROR_ACCESS_LOST"));
    }

    #[test]
    fn test_capture_error_timeout_display() {
        let msg = CaptureError::Timeout(Duration::from_millis(100)).to_string();
        assert!(msg.contains("timed out"));
    }

    // ── create_capturer ───────────────────────────────────────────────────────

    #[cfg(all(not(target_os = "windows"), not(feature = "fallback")))]
    #[test]
    fn test_create_capturer_unsupported_on_non_windows() {
        let result = create_capturer(CaptureConfig::default());
        assert!(matches!(result, Err(CaptureError::UnsupportedPlatform)));
    }

    #[cfg(all(not(target_os = "windows"), feature = "fallback"))]
    #[test]
    fn test_create_capturer_returns_scrap_on_non_windows_with_fallback() {
        let result = create_capturer(CaptureConfig::default());
        // Success depends on display/permission availability; just verify
        // that we don't get `UnsupportedPlatform`.
        match result {
            Ok(capturer) => {
                let (w, h) = capturer.resolution();
                assert!(w > 0);
                assert!(h > 0);
            }
            Err(CaptureError::InitializationFailed(_)) => {
                // Expected on headless CI or without screen-recording permission.
            }
            Err(other) => panic!("unexpected error variant: {other}"),
        }
    }
}
