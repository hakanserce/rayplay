pub mod capture;

#[cfg(target_os = "windows")]
pub mod dxgi_capture;

pub use capture::{CaptureConfig, CaptureError, CapturedFrame, ScreenCapturer, create_capturer};
