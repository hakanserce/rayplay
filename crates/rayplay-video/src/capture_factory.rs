use crate::capture::{CaptureConfig, CaptureError, ScreenCapturer};
use crate::pipeline_mode::PipelineMode;

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
pub fn create_capturer(
    config: CaptureConfig,
    mode: PipelineMode,
) -> Result<Box<dyn ScreenCapturer>, CaptureError> {
    if mode == PipelineMode::Software {
        #[cfg(feature = "fallback")]
        {
            use crate::scrap_capture::ScrapCapturer;
            return ScrapCapturer::new(config).map(|c| Box::new(c) as Box<dyn ScreenCapturer>);
        }
        #[cfg(not(feature = "fallback"))]
        {
            let _ = config;
            return Err(CaptureError::UnsupportedPlatform);
        }
    }

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

    #[cfg(all(not(target_os = "windows"), not(feature = "fallback")))]
    #[test]
    fn test_create_capturer_unsupported_on_non_windows() {
        let result = create_capturer(CaptureConfig::default(), PipelineMode::Auto);
        assert!(matches!(result, Err(CaptureError::UnsupportedPlatform)));
    }

    #[cfg(all(not(target_os = "windows"), feature = "fallback"))]
    #[test]
    fn test_create_capturer_returns_scrap_on_non_windows_with_fallback() {
        let result = create_capturer(CaptureConfig::default(), PipelineMode::Auto);
        match result {
            Ok(capturer) => {
                let (w, h) = capturer.resolution();
                assert!(w > 0);
                assert!(h > 0);
            }
            Err(CaptureError::InitializationFailed(_)) => {}
            Err(other) => panic!("unexpected error variant: {other}"),
        }
    }

    #[cfg(feature = "fallback")]
    #[test]
    fn test_create_capturer_software_mode_uses_scrap() {
        let result = create_capturer(CaptureConfig::default(), PipelineMode::Software);
        match result {
            Ok(capturer) => {
                let (w, h) = capturer.resolution();
                assert!(w > 0);
                assert!(h > 0);
            }
            Err(CaptureError::InitializationFailed(_)) => {}
            Err(other) => panic!("unexpected error variant: {other}"),
        }
    }

    #[cfg(not(feature = "fallback"))]
    #[test]
    fn test_create_capturer_software_mode_unsupported_without_fallback() {
        let result = create_capturer(CaptureConfig::default(), PipelineMode::Software);
        assert!(matches!(result, Err(CaptureError::UnsupportedPlatform)));
    }
}
