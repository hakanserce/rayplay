//! Host-side pairing/auth glue for the CLI (UC-016).
//!
//! Wires together the library-level pairing functions with the CLI's
//! accept loop.  Excluded from coverage because it performs trust-DB
//! persistence and PIN display I/O.

use anyhow::Result;
use rayplay_core::pairing::TrustDatabase;
use rayplay_core::session::{ClientIntent, ControlMessage};
use rayplay_network::{QuicVideoTransport, host_auth_challenge, host_pairing};
use tokio_util::sync::CancellationToken;

use crate::host::{HostConfig, stream};

/// Authenticates the client via challenge-response or PIN pairing, then streams.
pub(crate) async fn authenticate_and_stream(
    transport: QuicVideoTransport,
    config: HostConfig,
    trust_db: std::sync::Arc<tokio::sync::Mutex<TrustDatabase>>,
    token: CancellationToken,
) -> Result<()> {
    let mut control = transport
        .accept_control()
        .await
        .map_err(|e| anyhow::anyhow!("failed to accept control channel: {e}"))?;

    // Wait for ClientHello first to determine intent
    let intent = match control.recv_msg("hello").await {
        Ok(ControlMessage::ClientHello(intent)) => intent,
        Ok(other) => {
            return Err(anyhow::anyhow!("expected ClientHello, got {other:?}"));
        }
        Err(e) => {
            return Err(anyhow::anyhow!("failed to receive ClientHello: {e}"));
        }
    };

    match intent {
        ClientIntent::Auth => {
            // Try authentication
            let mut db = trust_db.lock().await;
            match host_auth_challenge(&mut control, &mut db).await {
                Ok(client) => {
                    tracing::info!(client_id = %client.client_id, "Trusted client authenticated");
                    drop(db);
                    save_trust_db_if_possible(&trust_db).await;
                    stream(transport, config, token).await
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Authentication failed");
                    Err(anyhow::anyhow!("authentication failed: {e}"))
                }
            }
        }
        ClientIntent::Pair => {
            // Perform PIN pairing
            let pin = rayplay_core::pairing::generate_pin();
            tracing::info!("────────────────────────────────────");
            tracing::info!("  Pairing PIN: {pin}");
            tracing::info!("  Enter this PIN on the client.");
            tracing::info!("────────────────────────────────────");

            let client = {
                let mut db = trust_db.lock().await;
                host_pairing(&mut control, &pin, &mut db, "unknown-client")
                    .await
                    .map_err(|e| anyhow::anyhow!("pairing failed: {e}"))?
            };

            tracing::info!(client_id = %client.client_id, "Client paired successfully");
            save_trust_db_if_possible(&trust_db).await;

            stream(transport, config, token).await
        }
    }
}

/// Best-effort save of the trust database to disk.
async fn save_trust_db_if_possible(trust_db: &std::sync::Arc<tokio::sync::Mutex<TrustDatabase>>) {
    let db = trust_db.lock().await;
    if let Err(e) = rayplay_network::trust_store::save_trust_db(&db) {
        tracing::warn!(error = %e, "Failed to persist trust database");
    }
}
