//! macOS screen capture via Apple's `ScreenCaptureKit` framework.
//!
//! Replaces the `scrap`-based capturer on macOS 12.3+.  Uses
//! `SCStream` with a callback that pushes frames into a bounded
//! channel, bridging the callback-based API to the synchronous
//! [`ScreenCapturer`] trait.
//!
//! Excluded from coverage: platform-specific capture code that
//! requires a real display.

use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, TrySendError};
use screencapturekit::cv::CVPixelBufferLockFlags;
use screencapturekit::prelude::*;

use crate::capture::{CaptureConfig, CaptureError, CapturedFrame, ScreenCapturer, build_frame};

struct FrameHandler {
    tx: Sender<Vec<u8>>,
}

impl SCStreamOutputTrait for FrameHandler {
    fn did_output_sample_buffer(&self, sample: CMSampleBuffer, _of_type: SCStreamOutputType) {
        let Some(pixel_buffer) = sample.image_buffer() else {
            return;
        };

        let Ok(lock) = pixel_buffer.lock(CVPixelBufferLockFlags::READ_ONLY) else {
            return;
        };

        let data = lock.as_slice().to_vec();

        // Non-blocking send — drop the frame if the consumer is behind.
        match self.tx.try_send(data) {
            Ok(()) | Err(TrySendError::Full(_)) => {}
            Err(TrySendError::Disconnected(_)) => {}
        }
    }
}

pub struct SckCapturer {
    _stream: Arc<SCStream>,
    rx: Receiver<Vec<u8>>,
    width: u32,
    height: u32,
    timeout: Duration,
}

impl SckCapturer {
    /// Creates a new `SckCapturer` targeting the primary display.
    ///
    /// # Errors
    ///
    /// Returns [`CaptureError::InitializationFailed`] if the display
    /// cannot be enumerated or the capture stream cannot start.
    pub fn new(config: CaptureConfig) -> Result<Self, CaptureError> {
        let content = SCShareableContent::get()
            .map_err(|e| CaptureError::InitializationFailed(format!("shareable content: {e}")))?;

        let displays = content.displays();
        let display = displays
            .first()
            .ok_or_else(|| CaptureError::InitializationFailed("no display found".to_string()))?;

        #[allow(clippy::cast_sign_loss)]
        let width = display.width() as u32;
        #[allow(clippy::cast_sign_loss)]
        let height = display.height() as u32;

        let stream_config = SCStreamConfiguration::new()
            .with_width(display.width())
            .with_height(display.height())
            .with_pixel_format(PixelFormat::BGRA)
            .with_shows_cursor(true);

        let filter = SCContentFilter::create()
            .with_display(display)
            .with_excluding_windows(&[])
            .build();

        let (tx, rx) = crossbeam_channel::bounded(1);

        let handler = FrameHandler { tx };
        let mut stream = SCStream::new(&filter, &stream_config);
        stream.add_output_handler(handler, SCStreamOutputType::Screen);
        stream
            .start_capture()
            .map_err(|e| CaptureError::InitializationFailed(format!("start capture: {e}")))?;

        let timeout = Duration::from_millis(u64::from(config.acquire_timeout_ms));

        Ok(Self {
            _stream: Arc::new(stream),
            rx,
            width,
            height,
            timeout,
        })
    }
}

impl ScreenCapturer for SckCapturer {
    fn capture_frame(&mut self) -> Result<CapturedFrame, CaptureError> {
        let data = self.rx.recv_timeout(self.timeout).map_err(|e| match e {
            crossbeam_channel::RecvTimeoutError::Timeout => CaptureError::Timeout(self.timeout),
            crossbeam_channel::RecvTimeoutError::Disconnected => {
                CaptureError::AcquireFailed("capture stream disconnected".to_string())
            }
        })?;

        Ok(build_frame(data, self.width, self.height))
    }

    fn resolution(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}
