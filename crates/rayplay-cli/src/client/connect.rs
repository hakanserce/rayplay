//! QUIC connection setup for the `rayview` client (UC-007, UC-008).

use std::{future::Future, net::SocketAddr};

use anyhow::Result;
use rayplay_network::QuicVideoTransport;
use tokio_util::sync::CancellationToken;

/// Connects to a `RayPlay` host and calls `on_connect` with the established transport.
///
/// Uses `select!` so a shutdown signal cancels the connection attempt before it
/// completes — mirrors the `serve_with_handler` pattern on the host side.
///
/// # Errors
///
/// Returns `Err` if the QUIC handshake fails or if `on_connect` returns an error.
pub(crate) async fn connect_with_handler<F, Fut>(
    server_addr: SocketAddr,
    server_cert: Vec<u8>,
    token: CancellationToken,
    on_connect: F,
) -> Result<()>
where
    F: FnOnce(QuicVideoTransport, CancellationToken) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    let connect_result = tokio::select! {
        () = token.cancelled() => None,
        result = QuicVideoTransport::connect(server_addr, server_cert) => Some(result),
    };

    match connect_result {
        None => {
            tracing::info!("Shutdown before connection completed");
            Ok(())
        }
        Some(Ok(transport)) => {
            tracing::info!(addr = %server_addr, "Connected to RayPlay host");
            on_connect(transport, token).await
        }
        Some(Err(e)) => Err(anyhow::anyhow!("connection to {server_addr} failed: {e}")),
    }
}

/// Wraps `connect_with_handler` in a retry loop with exponential backoff.
///
/// Retries on connection failure until `token` is cancelled or
/// `reconnect_timeout` is exceeded. Backoff starts at 500ms and doubles up
/// to a 10s cap, resetting on each successful connection.
///
/// A `reconnect_timeout` of `Duration::ZERO` means infinite retries.
pub(crate) async fn connect_with_reconnect<F, Fut>(
    server_addr: SocketAddr,
    server_cert: Vec<u8>,
    reconnect_timeout: std::time::Duration,
    token: CancellationToken,
    on_connect: F,
) -> Result<()>
where
    F: Fn(QuicVideoTransport, CancellationToken) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    const INITIAL_BACKOFF_MS: u64 = 500;
    const MAX_BACKOFF_MS: u64 = 10_000;

    let mut backoff_ms = INITIAL_BACKOFF_MS;
    let mut disconnect_start: Option<std::time::Instant> = None;

    loop {
        if token.is_cancelled() {
            return Ok(());
        }

        let child = token.child_token();
        let result =
            connect_with_handler(server_addr, server_cert.clone(), child, &on_connect).await;

        match result {
            Ok(()) => {
                backoff_ms = INITIAL_BACKOFF_MS;
                disconnect_start = None;
                tracing::info!(
                    state = "Reconnecting",
                    "Disconnected from host, will reconnect"
                );
            }
            Err(e) => {
                if token.is_cancelled() {
                    return Ok(());
                }

                let start = *disconnect_start.get_or_insert_with(std::time::Instant::now);
                if !reconnect_timeout.is_zero() && start.elapsed() >= reconnect_timeout {
                    tracing::info!(
                        state = "Disconnected",
                        "Reconnect timeout exceeded, giving up"
                    );
                    return Err(anyhow::anyhow!(
                        "reconnect timeout exceeded after {reconnect_timeout:?}"
                    ));
                }

                tracing::info!(state = "Reconnecting", error = %e, backoff_ms, "Connection failed, retrying");
            }
        }

        if token.is_cancelled() {
            return Ok(());
        }

        tokio::select! {
            () = token.cancelled() => return Ok(()),
            () = tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)) => {}
        }

        backoff_ms = (backoff_ms * 2).min(MAX_BACKOFF_MS);
    }
}

/// Connects to the host in `config` and runs the full receive-decode pipeline
/// with automatic reconnection on failure.
///
/// If `config.pair` is set, uses insecure TLS + SPAKE2 PIN pairing.
/// Otherwise tries trusted-client auth with a saved key, falling back to
/// cert-based connection.
///
/// # Errors
///
/// Returns an error if the certificate cannot be read or pairing fails.
#[cfg(target_os = "macos")]
pub async fn connect(
    config: super::config::ClientConfig,
    frame_tx: crossbeam_channel::Sender<rayplay_video::DecodedFrame>,
    token: CancellationToken,
) -> Result<()> {
    use rayplay_video::{Codec, create_decoder};

    let server_addr = config.server_addr;
    let pipeline_mode = config.pipeline_mode;
    let reconnect_timeout = config.reconnect_timeout;

    if config.pair {
        // Pairing mode: insecure connect + SPAKE2
        let transport = QuicVideoTransport::connect_insecure(server_addr)
            .await
            .map_err(|e| anyhow::anyhow!("insecure connect failed: {e}"))?;

        tracing::info!("Connected to host (insecure mode for pairing)");

        let mut control = transport
            .open_control()
            .await
            .map_err(|e| anyhow::anyhow!("failed to open control channel: {e}"))?;

        // Prompt user for PIN
        tracing::info!("Enter the 6-digit PIN shown on the host:");
        let mut pin = String::new();
        std::io::stdin()
            .read_line(&mut pin)
            .map_err(|e| anyhow::anyhow!("failed to read PIN: {e}"))?;
        let pin = pin.trim().to_string();

        let signing_key = rayplay_network::client_pairing(&mut control, &pin)
            .await
            .map_err(|e| anyhow::anyhow!("pairing failed: {e}"))?;

        tracing::info!("Pairing successful! Saving client key.");
        rayplay_network::client_key_store::save_client_key(&signing_key)
            .map_err(|e| anyhow::anyhow!("failed to save client key: {e}"))?;

        // After pairing, run the decode pipeline on this connection
        let decoder = create_decoder(Codec::Hevc, pipeline_mode)
            .map_err(|e| anyhow::anyhow!("decoder initialisation failed: {e}"))?;
        super::receive::run_receive_loop(transport, decoder, frame_tx, token).await
    } else {
        // Normal mode: cert-based connect with reconnect
        let cert_bytes = config.load_cert_bytes()?;

        connect_with_reconnect(
            server_addr,
            cert_bytes,
            reconnect_timeout,
            token,
            |transport, child| {
                let frame_tx = frame_tx.clone();
                async move {
                    let decoder = create_decoder(Codec::Hevc, pipeline_mode)
                        .map_err(|e| anyhow::anyhow!("decoder initialisation failed: {e}"))?;
                    super::receive::run_receive_loop(transport, decoder, frame_tx, child).await
                }
            },
        )
        .await
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;
