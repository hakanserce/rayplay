//! QUIC connection setup for the `rayview` client (UC-007).

use std::{future::Future, net::SocketAddr};

use anyhow::Result;
use rayplay_network::QuicVideoTransport;
use rustls::pki_types::CertificateDer;

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
    mut shutdown: tokio::sync::oneshot::Receiver<()>,
    on_connect: F,
) -> Result<()>
where
    F: FnOnce(QuicVideoTransport, tokio::sync::oneshot::Receiver<()>) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    let cert = CertificateDer::from(server_cert);

    let connect_result = tokio::select! {
        _ = &mut shutdown => None,
        result = QuicVideoTransport::connect(server_addr, cert) => Some(result),
    };

    match connect_result {
        None => {
            tracing::info!("Shutdown before connection completed");
            Ok(())
        }
        Some(Ok(transport)) => {
            tracing::info!(addr = %server_addr, "Connected to RayPlay host");
            on_connect(transport, shutdown).await
        }
        Some(Err(e)) => Err(anyhow::anyhow!("connection to {server_addr} failed: {e}")),
    }
}

/// Connects to the host in `config` and runs the full receive-decode pipeline.
///
/// Reads the server certificate from `config.cert_path`, establishes the QUIC
/// connection, and loops: receive → decode → forward frame.
///
/// # Errors
///
/// Returns an error if the certificate cannot be read, the connection fails, or
/// the receive loop encounters a fatal network error.
#[cfg(target_os = "macos")]
pub async fn connect(
    config: super::config::ClientConfig,
    frame_tx: crossbeam_channel::Sender<rayplay_video::DecodedFrame>,
    shutdown: tokio::sync::oneshot::Receiver<()>,
) -> Result<()> {
    use rayplay_video::VtDecoder;

    let cert_bytes = config.load_cert_bytes()?;
    let server_addr = config.server_addr;

    connect_with_handler(
        server_addr,
        cert_bytes,
        shutdown,
        |transport, shutdown| async move {
            let decoder = VtDecoder::new()
                .map_err(|e| anyhow::anyhow!("decoder initialisation failed: {e}"))?;
            super::receive::run_receive_loop(transport, Box::new(decoder), frame_tx, shutdown).await
        },
    )
    .await
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::test_helper::loopback_listener;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_connect_with_handler_shutdown_before_connect() {
        let (listener, cert_bytes, addr) = loopback_listener();
        let _guard = tokio::spawn(async move { listener.accept().await });
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tx.send(()).unwrap();
        assert!(
            connect_with_handler(addr, cert_bytes, rx, |_t, _s| async { Ok(()) })
                .await
                .is_ok()
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_connect_with_handler_connection_failure_returns_error() {
        let (listener, _correct, addr) = loopback_listener();
        let (_, wrong_cert, _) = loopback_listener();
        let _server = tokio::spawn(async move { listener.accept().await });
        let (_tx, rx) = tokio::sync::oneshot::channel::<()>();
        let err = connect_with_handler(addr, wrong_cert, rx, |_t, _s| async { Ok(()) })
            .await
            .unwrap_err();
        assert!(err.to_string().contains("connection"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_connect_with_handler_calls_on_connect_on_success() {
        let (listener, cert_bytes, addr) = loopback_listener();
        let _server = tokio::spawn(async move { listener.accept().await });
        let (_tx, rx) = tokio::sync::oneshot::channel::<()>();
        assert!(
            connect_with_handler(addr, cert_bytes, rx, |_t, _s| async { Ok(()) })
                .await
                .is_ok()
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_connect_with_handler_propagates_handler_error() {
        let (listener, cert_bytes, addr) = loopback_listener();
        let _server = tokio::spawn(async move { listener.accept().await });
        let (_tx, rx) = tokio::sync::oneshot::channel::<()>();
        let err = connect_with_handler(addr, cert_bytes, rx, |_t, _s| async {
            Err(anyhow::anyhow!("handler failed"))
        })
        .await
        .unwrap_err();
        assert!(err.to_string().contains("handler failed"));
    }

    #[cfg(target_os = "macos")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_connect_cert_missing_returns_error() {
        use super::super::config::ClientConfig;
        let config = ClientConfig {
            server_addr: "127.0.0.1:5000".parse().unwrap(),
            cert_path: "/nonexistent/cert.der".into(),
            width: 1280,
            height: 720,
        };
        let (frame_tx, _rx) = crossbeam_channel::bounded(4);
        let (_stx, shutdown_rx) = tokio::sync::oneshot::channel();
        let err = connect(config, frame_tx, shutdown_rx).await.unwrap_err();
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
            width: 1280,
            height: 720,
        };
        let (frame_tx, _rx) = crossbeam_channel::bounded(4);
        let (stx, shutdown_rx) = tokio::sync::oneshot::channel();
        stx.send(()).unwrap();
        assert!(connect(config, frame_tx, shutdown_rx).await.is_ok());
    }

    #[cfg(target_os = "macos")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_connect_handler_runs_until_shutdown() {
        use super::super::config::ClientConfig;
        let (listener, cert, addr) = loopback_listener();
        let _server = tokio::spawn(async move { listener.accept().await });
        let dir = tempfile::tempdir().unwrap();
        let cert_path = dir.path().join("server.der");
        std::fs::write(&cert_path, &cert).unwrap();
        let config = ClientConfig {
            server_addr: addr,
            cert_path,
            width: 1280,
            height: 720,
        };
        let (frame_tx, _rx) = crossbeam_channel::bounded(4);
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

        // Spawn connect() without pre-sending shutdown so the handler closure
        // (VtDecoder creation + receive loop) is actually entered.
        let task = tokio::spawn(connect(config, frame_tx, shutdown_rx));
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        shutdown_tx.send(()).unwrap();
        assert!(task.await.unwrap().is_ok());
    }
}
