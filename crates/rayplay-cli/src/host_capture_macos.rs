//! macOS-specific capture initialization for the host.
//!
//! Checks Screen Recording permission and initializes the capture pipeline
//! via `ScreenCaptureKit`.  Excluded from coverage — platform I/O that
//! requires a real display.

use anyhow::Result;
use rayplay_network::QuicVideoTransport;
use rayplay_video::encoder::EncoderConfig;
use tokio_util::sync::CancellationToken;

use crate::host::{HostConfig, stream_with_pipeline};

/// Checks Screen Recording permission, then creates the capture and encode
/// pipeline and streams to the connected client.
pub(crate) async fn stream(
    transport: QuicVideoTransport,
    config: HostConfig,
    token: CancellationToken,
) -> Result<()> {
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
        .with_bitrate(config.encoder_config.bitrate);
    let encoder = create_encoder(enc_config, config.pipeline_mode).map_err(anyhow::Error::from)?;

    stream_with_pipeline(transport, capturer, encoder, token).await
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
