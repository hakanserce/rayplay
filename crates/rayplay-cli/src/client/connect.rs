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
                tracing::info!(state = "Reconnecting", "Disconnected from host, will reconnect");
            }
            Err(e) => {
                if token.is_cancelled() {
                    return Ok(());
                }

                let start = *disconnect_start.get_or_insert_with(std::time::Instant::now);
                if !reconnect_timeout.is_zero() && start.elapsed() >= reconnect_timeout {
                    tracing::info!(state = "Disconnected", "Reconnect timeout exceeded, giving up");
                    return Err(anyhow::anyhow!("reconnect timeout exceeded after {reconnect_timeout:?}"));
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

        connect_with_reconnect(server_addr, cert_bytes, reconnect_timeout, token, |transport, child| {
            let frame_tx = frame_tx.clone();
            async move {
                let decoder = create_decoder(Codec::Hevc, pipeline_mode)
                    .map_err(|e| anyhow::anyhow!("decoder initialisation failed: {e}"))?;
                super::receive::run_receive_loop(transport, decoder, frame_tx, child).await
            }
        })
        .await
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::test_helper::loopback_listener;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_connect_with_handler_shutdown_before_connect() {
        let (listener, cert_bytes, addr) = loopback_listener();
        let _server = tokio::spawn(async move { listener.accept().await });
        let token = CancellationToken::new();
        token.cancel();
        assert!(
            connect_with_handler(addr, cert_bytes, token, |_t, _s| async { Ok(()) })
                .await
                .is_ok()
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_connect_with_handler_connection_failure_returns_error() {
        let (listener, _correct, addr) = loopback_listener();
        let (_, wrong_cert, _) = loopback_listener();
        let _server = tokio::spawn(async move { listener.accept().await });
        let token = CancellationToken::new();
        let err = connect_with_handler(addr, wrong_cert, token, |_t, _s| async { Ok(()) })
            .await
            .unwrap_err();
        assert!(err.to_string().contains("connection"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_connect_with_handler_calls_on_connect_on_success() {
        let (listener, cert_bytes, addr) = loopback_listener();
        let _server = tokio::spawn(async move { listener.accept().await });
        let token = CancellationToken::new();
        assert!(
            connect_with_handler(addr, cert_bytes, token, |_t, _s| async { Ok(()) })
                .await
                .is_ok()
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_connect_with_handler_propagates_handler_error() {
        let (listener, cert_bytes, addr) = loopback_listener();
        let _server = tokio::spawn(async move { listener.accept().await });
        let token = CancellationToken::new();
        let err = connect_with_handler(addr, cert_bytes, token, |_t, _s| async {
            Err(anyhow::anyhow!("handler failed"))
        })
        .await
        .unwrap_err();
        assert!(err.to_string().contains("handler failed"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_connect_with_reconnect_shutdown_before_first_attempt() {
        let (_listener, cert_bytes, addr) = loopback_listener();
        let token = CancellationToken::new();
        token.cancel();
        assert!(
            connect_with_reconnect(addr, cert_bytes, std::time::Duration::ZERO, token, |_t, _s| async { Ok(()) })
                .await
                .is_ok()
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_connect_with_reconnect_retries_on_failure() {
        let (listener, _correct, addr) = loopback_listener();
        let (_, wrong_cert, _) = loopback_listener();
        // Server accepts but wrong cert causes handshake failure → retry.
        let _server = tokio::spawn(async move {
            loop {
                let _ = listener.accept().await;
            }
        });
        let token = CancellationToken::new();
        let token2 = token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(800)).await;
            token2.cancel();
        });
        assert!(
            connect_with_reconnect(addr, wrong_cert, std::time::Duration::ZERO, token, |_t, _s| async { Ok(()) })
                .await
                .is_ok()
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_connect_with_reconnect_resets_backoff_on_success() {
        let (listener, cert_bytes, addr) = loopback_listener();
        let _server = tokio::spawn(async move {
            loop {
                let _ = listener.accept().await;
            }
        });
        let token = CancellationToken::new();
        let token2 = token.clone();
        // Handler succeeds → backoff resets → cancel on 2nd connect to exit.
        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count = call_count.clone();
        let result = tokio::spawn(async move {
            connect_with_reconnect(addr, cert_bytes, std::time::Duration::ZERO, token, move |_t, _s| {
                let c = count.clone();
                let t = token2.clone();
                async move {
                    if c.fetch_add(1, std::sync::atomic::Ordering::SeqCst) >= 1 {
                        t.cancel();
                    }
                    Ok(())
                }
            })
            .await
        })
        .await
        .unwrap();
        assert!(result.is_ok());
        assert!(call_count.load(std::sync::atomic::Ordering::SeqCst) >= 2);
    }

    #[cfg(target_os = "macos")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_connect_cert_missing_returns_error() {
        use super::super::config::ClientConfig;
        let config = ClientConfig {
            server_addr: "127.0.0.1:5000".parse().unwrap(),
            cert_path: "/nonexistent/cert.der".into(),
            pair: false,
            width: 1280,
            height: 720,
            pipeline_mode: rayplay_video::PipelineMode::Auto,
            reconnect_timeout: std::time::Duration::from_secs(30),
        };
        let (frame_tx, _rx) = crossbeam_channel::bounded(4);
        let token = CancellationToken::new();
        let err = connect(config, frame_tx, token).await.unwrap_err();
        assert!(err.to_string().contains("failed to read"));
    }

    #[cfg(target_os = "macos")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_connect_succeeds_with_valid_cert_and_immediate_shutdown() {
        use super::super::config::ClientConfig;
        let (listener, cert, addr) = loopback_listener();
        let _server = tokio::spawn(async move { listener.accept().await });
        let dir = tempfile::tempdir().unwrap();
        let cert_path = dir.path().join("server.der");
        std::fs::write(&cert_path, &cert).unwrap();
        let config = ClientConfig {
            server_addr: addr,
            cert_path,
            pair: false,
            width: 1280,
            height: 720,
            pipeline_mode: rayplay_video::PipelineMode::Auto,
            reconnect_timeout: std::time::Duration::from_secs(30),
        };
        let (frame_tx, _rx) = crossbeam_channel::bounded(4);
        let token = CancellationToken::new();
        token.cancel();
        assert!(connect(config, frame_tx, token).await.is_ok());
    }

    #[cfg(target_os = "macos")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_connect_handler_runs_until_shutdown() {
        use super::super::config::ClientConfig;
        let (listener, cert, addr) = loopback_listener();
        let server_task = tokio::spawn(async move { listener.accept().await });
        let dir = tempfile::tempdir().unwrap();
        let cert_path = dir.path().join("server.der");
        std::fs::write(&cert_path, &cert).unwrap();
        let config = ClientConfig {
            server_addr: addr,
            cert_path,
            pair: false,
            width: 1280,
            height: 720,
            pipeline_mode: rayplay_video::PipelineMode::Auto,
            reconnect_timeout: std::time::Duration::from_secs(30),
        };
        let (frame_tx, _rx) = crossbeam_channel::bounded(4);
        let token = CancellationToken::new();
        let task = tokio::spawn(connect(config, frame_tx, token.clone()));

        let _server = server_task.await.unwrap();
        token.cancel();
        assert!(task.await.unwrap().is_ok());
    }
}
