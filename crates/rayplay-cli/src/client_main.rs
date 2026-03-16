//! `rayview` binary — entry point for the `RayPlay` client viewer (UC-007).

mod client;

use std::sync::{Arc, Mutex};

use anyhow::Result;
use clap::Parser;
use client::{ClientArgs, ClientConfig};
use rayplay_video::{DecodedFrame, RenderWindow};

/// Bounded capacity of the decoded-frame channel between the network thread
/// and the `winit` render loop.  Keeping this small (2) ensures the renderer
/// always works on the most recent frame rather than draining a backlog.
const DEFAULT_FRAME_CHANNEL_CAPACITY: usize = 2;

// llvm-cov:excl-start

/// Supported on macOS only; other platforms bail with a clear message.
#[cfg(not(target_os = "macos"))]
fn main() -> Result<()> {
    anyhow::bail!("RayView is currently only supported on macOS")
}

#[cfg(target_os = "macos")]
fn main() -> Result<()> {
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
        "RayView connecting"
    );

    // Extract window dimensions before `config` is moved into the network thread.
    let width = config.width;
    let height = config.height;

    let (frame_tx, frame_rx) =
        crossbeam_channel::bounded::<DecodedFrame>(DEFAULT_FRAME_CHANNEL_CAPACITY);
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    // Wrap in Arc<Mutex<Option>> so both Ctrl+C (net thread) and window-close
    // (main thread) can fire the shutdown signal exactly once.
    let shutdown = Arc::new(Mutex::new(Some(shutdown_tx)));
    let ctrlc_shutdown = Arc::clone(&shutdown);

    // Spawn a background OS thread that owns the tokio runtime and QUIC
    // connection.  The winit event loop must run on the main thread (AppKit
    // requirement on macOS), so networking runs on a dedicated thread instead.
    let net_thread = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        rt.block_on(async move {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    tracing::info!("Ctrl+C received, disconnecting");
                    if let Some(tx) = ctrlc_shutdown.lock().expect("shutdown lock").take() {
                        let _ = tx.send(());
                    }
                    // connect future is cancelled here; quinn sends CONNECTION_CLOSE via Drop.
                }
                result = client::connect(config, frame_tx, shutdown_rx) => {
                    if let Err(e) = result {
                        tracing::error!(error = %e, "Connection error");
                    }
                }
            }
        });
    });

    // Main thread: runs the winit event loop until the window is closed.
    let render_result = RenderWindow::new("RayView", width, height).run(frame_rx);
    if let Err(ref e) = render_result {
        tracing::error!(error = %e, "Render window error");
    }

    // Window closed — signal the networking thread to stop, then wait for it.
    if let Some(tx) = shutdown.lock().expect("shutdown lock").take() {
        let _ = tx.send(());
    }
    let _ = net_thread.join();

    render_result.map_err(|e| anyhow::anyhow!("render window: {e}"))
}

// llvm-cov:excl-stop
