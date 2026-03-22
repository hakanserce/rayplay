//! `rayview` binary — entry point for the `RayPlay` client viewer (UC-007, UC-008).

mod client;

use anyhow::Result;
use clap::Parser;
use client::{ClientArgs, ClientConfig};
use rayplay_video::{DecodedFrame, RenderWindow};

/// Supported on macOS only; other platforms bail with a clear message.
#[cfg(not(target_os = "macos"))]
fn main() -> Result<()> {
    anyhow::bail!("RayView is currently only supported on macOS")
}

#[cfg(target_os = "macos")]
fn main() -> Result<()> {
    use tokio_util::sync::CancellationToken;

    /// Bounded capacity of the decoded-frame channel between the network thread
    /// and the `winit` render loop.  Keeping this small (2) ensures the renderer
    /// always works on the most recent frame rather than draining a backlog.
    const DEFAULT_FRAME_CHANNEL_CAPACITY: usize = 2;

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("rayview=info,warn")),
        )
        .init();

    let args = ClientArgs::parse();
    let config = ClientConfig::from_args(&args)?;

    tracing::info!(
        addr    = %config.server_addr,
        width   = config.width,
        height  = config.height,
        pair    = config.pair,
        "RayView connecting"
    );

    let width = config.width;
    let height = config.height;

    let (frame_tx, frame_rx) =
        crossbeam_channel::bounded::<DecodedFrame>(DEFAULT_FRAME_CHANNEL_CAPACITY);
    let token = CancellationToken::new();
    let ctrl_token = token.clone();

    // Spawn a background OS thread that owns the tokio runtime and QUIC
    // connection.  The winit event loop must run on the main thread (AppKit
    // requirement on macOS), so networking runs on a dedicated thread instead.
    let net_thread = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        rt.block_on(async move {
            let ctrl_token2 = ctrl_token.clone();
            tokio::spawn(async move {
                let _ = tokio::signal::ctrl_c().await;
                tracing::info!("Ctrl+C received, disconnecting");
                ctrl_token2.cancel();
            });
            if let Err(e) = client::connect(config, frame_tx, ctrl_token).await {
                tracing::error!(error = %e, "Connection error");
            }
        });
    });

    // Main thread: runs the winit event loop until the window is closed.
    let render_result = RenderWindow::new("RayView", width, height).run(frame_rx);
    if let Err(ref e) = render_result {
        tracing::error!(error = %e, "Render window error");
    }

    // Window closed — signal the networking thread to stop, then wait for it.
    token.cancel();
    let _ = net_thread.join();

    render_result.map_err(|e| anyhow::anyhow!("render window: {e}"))
}
