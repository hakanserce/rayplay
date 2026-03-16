//! `rayhost` binary — entry point for the `RayPlay` host streaming server (UC-006).

mod host;

use anyhow::Result;
use clap::Parser;
use rayplay_network::QuicVideoTransport;
use tracing_subscriber::EnvFilter;

use host::{HostArgs, HostConfig};

// llvm-cov:excl-start
#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("rayhost=info,warn")),
        )
        .init();

    let args = HostArgs::parse();
    let config = HostConfig::from_args(&args);

    // TODO(UC-pairing): distribute cert_der to the client via the discovery /
    // SPAKE2 pairing channel (ADR-007) so the client can authenticate the server.
    let (listener, _cert_der) = QuicVideoTransport::listen(config.bind_addr)
        .map_err(|e| anyhow::Error::from(e).context("failed to bind"))?;

    let addr = listener
        .local_addr()
        .map_err(|e| anyhow::Error::from(e).context("local_addr"))?;

    tracing::info!(
        addr = %addr,
        width  = config.encoder_config.width,
        height = config.encoder_config.height,
        fps    = config.encoder_config.fps,
        bitrate_bps = config.encoder_config.resolved_bitrate(),
        "RayHost listening — waiting for client"
    );

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("Ctrl+C received");
        let _ = shutdown_tx.send(());
    });

    host::serve(listener, config, shutdown_rx).await
}
// llvm-cov:excl-stop
