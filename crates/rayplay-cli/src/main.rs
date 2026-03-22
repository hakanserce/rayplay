//! `rayhost` binary — entry point for the `RayPlay` host streaming server (UC-006, UC-008, UC-016).

mod host;
mod host_pairing_glue;

use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use rayplay_network::QuicVideoTransport;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;

use host::{HostArgs, HostConfig};

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

    // Self-signed cert is generated in-memory; clients connect via insecure TLS
    // during SPAKE2 pairing (PIN provides authentication, not TLS certs).
    let (listener, _cert_der) = QuicVideoTransport::listen(config.bind_addr)
        .map_err(|e| anyhow::Error::from(e).context("failed to bind"))?;

    let addr = listener
        .local_addr()
        .map_err(|e| anyhow::Error::from(e).context("local_addr"))?;

    // Load or create trust database
    let trust_db = match rayplay_network::trust_store::load_trust_db() {
        Ok(db) => {
            let n = db.len();
            if n > 0 {
                tracing::info!(trusted_clients = n, "Loaded trust database");
            }
            db
        }
        Err(e) => {
            tracing::warn!(error = %e, "Failed to load trust database, starting fresh");
            rayplay_core::pairing::TrustDatabase::new()
        }
    };
    let trust_db = Arc::new(Mutex::new(trust_db));

    tracing::info!(
        addr = %addr,
        width  = config.encoder_config.width,
        height = config.encoder_config.height,
        fps    = config.encoder_config.fps,
        bitrate_bps = config.encoder_config.resolved_bitrate(),
        "RayHost listening — waiting for client"
    );

    let token = CancellationToken::new();
    let ctrl_token = token.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("Ctrl+C received");
        ctrl_token.cancel();
    });

    host::serve(listener, config, trust_db, token).await
}
