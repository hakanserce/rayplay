//! macOS-specific capture helpers for the host.
//!
//! Checks Screen Recording permission before capture starts.
//! Excluded from coverage — platform I/O that requires a real display.

use anyhow::Result;

/// Polls for macOS Screen Recording permission, prompting the user to grant it.
///
/// On the first check, opens System Settings to the Screen Recording pane.
/// Then polls every 2 seconds until the permission is granted.
pub(crate) async fn wait_for_screen_recording_permission() -> Result<()> {
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
