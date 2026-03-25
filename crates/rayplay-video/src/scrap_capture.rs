use std::time::Duration;

use scrap::{Capturer, Display};

use crate::capture::{CaptureConfig, CaptureError, CapturedFrame, ScreenCapturer, build_frame};

/// Thin abstraction over `scrap::Capturer` to allow unit-testing without a
/// real display.
trait FrameSource: Send {
    fn grab(&mut self) -> Result<Vec<u8>, std::io::Error>;
}

struct ScrapSource(Capturer);

// SAFETY: `scrap::Capturer` wraps platform-specific capture APIs that use raw
// pointers internally, preventing auto-`Send`. We ensure `ScrapSource` is only
// accessed from a single dedicated capture thread (same model as `DxgiCapture`).
unsafe impl Send for ScrapSource {}

impl FrameSource for ScrapSource {
    fn grab(&mut self) -> Result<Vec<u8>, std::io::Error> {
        self.0.frame().map(|f| f.to_vec())
    }
}

pub struct ScrapCapturer {
    source: Box<dyn FrameSource>,
    width: u32,
    height: u32,
}

impl ScrapCapturer {
    /// Creates a new `ScrapCapturer` targeting the primary display.
    ///
    /// Frame pacing is the caller's responsibility — `CaptureConfig` fields
    /// (`target_fps`, `acquire_timeout_ms`) are not used by the scrap backend
    /// because `scrap::Capturer::frame()` is non-blocking (returns `WouldBlock`
    /// when no new frame is available).
    ///
    /// # Errors
    ///
    /// Returns [`CaptureError::InitializationFailed`] if the primary display
    /// cannot be obtained or the capturer cannot be created.
    pub fn new(_config: CaptureConfig) -> Result<Self, CaptureError> {
        let display = Display::primary()
            .map_err(|e| CaptureError::InitializationFailed(format!("primary display: {e}")))?;
        #[allow(clippy::cast_possible_truncation)]
        let width = display.width() as u32;
        #[allow(clippy::cast_possible_truncation)]
        let height = display.height() as u32;
        let capturer = Capturer::new(display)
            .map_err(|e| CaptureError::InitializationFailed(format!("capturer: {e}")))?;
        Ok(Self {
            source: Box::new(ScrapSource(capturer)),
            width,
            height,
        })
    }
}

impl ScreenCapturer for ScrapCapturer {
    fn capture_frame(&mut self) -> Result<CapturedFrame, CaptureError> {
        let data = self.source.grab().map_err(|e| {
            if e.kind() == std::io::ErrorKind::WouldBlock {
                CaptureError::Timeout(Duration::from_millis(0))
            } else {
                CaptureError::AcquireFailed(e.to_string())
            }
        })?;
        Ok(build_frame(data, self.width, self.height))
    }

    fn resolution(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockSource {
        data: Vec<u8>,
        fail_kind: Option<std::io::ErrorKind>,
    }

    impl FrameSource for MockSource {
        fn grab(&mut self) -> Result<Vec<u8>, std::io::Error> {
            if let Some(kind) = self.fail_kind {
                Err(std::io::Error::new(kind, "mock error"))
            } else {
                Ok(self.data.clone())
            }
        }
    }

    fn make_test_capturer(width: u32, height: u32, data: Vec<u8>) -> ScrapCapturer {
        ScrapCapturer {
            source: Box::new(MockSource {
                data,
                fail_kind: None,
            }),
            width,
            height,
        }
    }

    fn make_failing_capturer() -> ScrapCapturer {
        ScrapCapturer {
            source: Box::new(MockSource {
                data: vec![],
                fail_kind: Some(std::io::ErrorKind::WouldBlock),
            }),
            width: 100,
            height: 100,
        }
    }

    #[test]
    fn test_resolution_returns_cached_values() {
        let capturer = make_test_capturer(1920, 1080, vec![]);
        assert_eq!(capturer.resolution(), (1920, 1080));
    }

    #[test]
    fn test_resolution_different_values() {
        let capturer = make_test_capturer(2560, 1440, vec![]);
        assert_eq!(capturer.resolution(), (2560, 1440));
    }

    #[test]
    fn test_capture_frame_returns_correct_dimensions() {
        let data = vec![0u8; 4 * 4 * 3]; // 4×3 BGRA
        let mut capturer = make_test_capturer(4, 3, data);
        let frame = capturer.capture_frame().expect("should succeed");
        assert_eq!(frame.width, 4);
        assert_eq!(frame.height, 3);
        assert_eq!(frame.stride, 16);
    }

    #[test]
    fn test_capture_frame_returns_pixel_data() {
        let data: Vec<u8> = (0..16).collect(); // 2×2 BGRA
        let mut capturer = make_test_capturer(2, 2, data.clone());
        let frame = capturer.capture_frame().expect("should succeed");
        assert_eq!(frame.data, data);
    }

    #[test]
    fn test_capture_frame_would_block_maps_to_timeout() {
        let mut capturer = make_failing_capturer();
        let err = capturer.capture_frame().expect_err("should fail");
        assert!(matches!(err, CaptureError::Timeout(_)));
    }

    #[test]
    fn test_capture_frame_other_error_maps_to_acquire_failed() {
        let mut capturer = ScrapCapturer {
            source: Box::new(MockSource {
                data: vec![],
                fail_kind: Some(std::io::ErrorKind::ConnectionRefused),
            }),
            width: 100,
            height: 100,
        };
        let err = capturer.capture_frame().expect_err("should fail");
        assert!(matches!(err, CaptureError::AcquireFailed(_)));
    }

    #[test]
    fn test_scrap_capturer_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<ScrapCapturer>();
    }

    #[test]
    fn test_capture_error_display_init() {
        let err = CaptureError::InitializationFailed("primary display: no display".to_string());
        assert!(err.to_string().contains("initialize"));
        assert!(err.to_string().contains("no display"));
    }

    #[test]
    fn test_capture_error_capturer_init() {
        let err = CaptureError::InitializationFailed("capturer: permission denied".to_string());
        assert!(err.to_string().contains("capturer"));
    }

    #[cfg(feature = "hw-codec-tests")]
    #[test]
    fn test_new_returns_ok_or_initialization_error() {
        let result = ScrapCapturer::new(CaptureConfig::default());
        match &result {
            Ok(c) => {
                let (w, h) = c.resolution();
                assert!(w > 0);
                assert!(h > 0);
            }
            Err(CaptureError::InitializationFailed(msg)) => {
                assert!(msg.contains("display") || msg.contains("capturer"));
            }
            Err(other) => panic!("unexpected error variant: {other}"),
        }
    }

    #[cfg(feature = "hw-codec-tests")]
    #[test]
    fn test_capture_frame_live() {
        let mut capturer = ScrapCapturer::new(CaptureConfig::default())
            .expect("hw-codec-tests requires display access");
        let (w, h) = capturer.resolution();
        for _ in 0..10 {
            match capturer.capture_frame() {
                Ok(frame) => {
                    assert_eq!(frame.width, w);
                    assert_eq!(frame.height, h);
                    assert!(frame.stride >= w * 4);
                    assert!(!frame.data.is_empty());
                    return;
                }
                Err(CaptureError::AcquireFailed(_)) => {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                Err(e) => panic!("unexpected error: {e}"),
            }
        }
    }
}
