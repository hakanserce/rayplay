//! macOS-specific capture initialization for the host.
//!
//! Checks Screen Recording permission and initializes the capture pipeline
//! via `ScreenCaptureKit`.  Excluded from coverage — platform I/O that
//! requires a real display.

use anyhow::Result;
use rayplay_video::{
    capture::ScreenCapturer,
    encoder::{EncoderConfig, VideoEncoder},
};

use crate::host::HostConfig;

/// Creates the capture and encoder pipeline but does not start streaming.
///
/// Checks Screen Recording permission first, then initializes the capturer
/// and encoder with the actual capture resolution.
///
/// # Errors
///
/// Returns an error if Screen Recording permission is denied, capture
/// initialization fails, or encoder creation fails.
pub(crate) async fn prepare_pipeline(
    config: &HostConfig,
) -> Result<(Box<dyn ScreenCapturer>, Box<dyn VideoEncoder>)> {
    use rayplay_video::{CaptureConfig, create_capturer, encoder::create_encoder};

    wait_for_screen_recording_permission().await?;

    let cap_config = CaptureConfig {
        target_fps: config.encoder_config.fps,
        acquire_timeout_ms: 100,
    };
    let capturer =
        create_capturer(cap_config, config.pipeline_mode).map_err(anyhow::Error::from)?;
    let (cap_width, cap_height) = capturer.resolution();

    let enc_config = EncoderConfig::new(cap_width, cap_height, config.encoder_config.fps)
        .with_bitrate(config.encoder_config.bitrate.clone());
    let encoder = create_encoder(enc_config, config.pipeline_mode).map_err(anyhow::Error::from)?;

    Ok((capturer, encoder))
}

/// Polls for macOS Screen Recording permission, prompting the user to grant it.
///
/// On the first check, opens System Settings to the Screen Recording pane.
/// Then polls every 2 seconds until the permission is granted.
async fn wait_for_screen_recording_permission() -> Result<()> {
    use rayplay_video::screen_permission_macos::{
        has_screen_recording_permission, request_screen_recording_permission,
    };

    if has_screen_recording_permission() {
        return Ok(());
    }

    tracing::warn!("Screen Recording permission is not granted.");
    tracing::warn!("Please enable it in System Settings > Privacy & Security > Screen Recording.");

    // Trigger the system prompt / open Settings pane.
    let _ = request_screen_recording_permission();

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        if has_screen_recording_permission() {
            tracing::info!("Screen Recording permission granted.");
            return Ok(());
        }
        tracing::info!("Waiting for Screen Recording permission...");
    }
}
