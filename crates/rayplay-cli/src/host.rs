//! `RayHost` server — CLI configuration and connection-accept loop (UC-006).

use std::{future::Future, net::SocketAddr};

use anyhow::Result;
use clap::Parser;
use rayplay_network::{QuicListener, QuicVideoTransport};
use rayplay_video::encoder::{Bitrate, EncoderConfig};

/// Command-line arguments for the `rayhost` binary.
#[derive(Parser, Debug, Clone)]
#[command(
    name = "rayhost",
    about = "RayPlay host streaming server",
    long_about = "Start a RayPlay host that listens for an incoming client \
                  connection and streams the captured display."
)]
pub struct HostArgs {
    /// UDP port to listen on.
    #[arg(short, long, default_value_t = 5000)]
    pub port: u16,

    /// Capture/stream width in pixels.
    #[arg(long, default_value_t = 1920)]
    pub width: u32,

    /// Capture/stream height in pixels.
    #[arg(long, default_value_t = 1080)]
    pub height: u32,

    /// Target frame rate.
    #[arg(long, default_value_t = 60)]
    pub fps: u32,

    /// Encoder bitrate in Mbps (0 = auto-compute from resolution and fps).
    #[arg(long, default_value_t = 0)]
    pub bitrate: u32,
}

/// Resolved server configuration derived from [`HostArgs`].
#[derive(Debug, Clone)]
pub struct HostConfig {
    /// Address the QUIC listener will bind to.
    pub bind_addr: SocketAddr,
    /// Video encoder settings derived from the CLI arguments.
    pub encoder_config: EncoderConfig,
}

impl HostConfig {
    /// Builds a [`HostConfig`] from the parsed CLI arguments.
    #[must_use]
    pub fn from_args(args: &HostArgs) -> Self {
        let bind_addr = SocketAddr::from(([0, 0, 0, 0], args.port));
        let bitrate = if args.bitrate == 0 {
            Bitrate::Auto
        } else {
            Bitrate::Mbps(args.bitrate)
        };
        let encoder_config =
            EncoderConfig::new(args.width, args.height, args.fps).with_bitrate(bitrate);
        Self {
            bind_addr,
            encoder_config,
        }
    }
}

// ── Accept loop ───────────────────────────────────────────────────────────────

/// Waits for one client connection (or a shutdown signal) and calls
/// `on_connect` with the established transport and the remaining shutdown
/// receiver.
///
/// Using a generic `on_connect` keeps the accept/shutdown logic testable
/// without requiring the real platform-specific streaming pipeline.
///
/// # Errors
///
/// Propagates errors from the QUIC handshake or from `on_connect`.
pub(crate) async fn serve_with_handler<F, Fut>(
    listener: QuicListener,
    mut shutdown: tokio::sync::oneshot::Receiver<()>,
    on_connect: F,
) -> Result<()>
where
    F: FnOnce(QuicVideoTransport, tokio::sync::oneshot::Receiver<()>) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    // Borrow `shutdown` mutably so we retain ownership after the select —
    // it is forwarded to the connection handler if a client connects first.
    let accept_result = tokio::select! {
        _ = &mut shutdown => None,
        result = listener.accept() => Some(result),
    };

    match accept_result {
        None => {
            tracing::info!("Shutdown signal received, stopping");
            Ok(())
        }
        Some(Ok(transport)) => {
            tracing::info!("Client connected");
            on_connect(transport, shutdown).await
        }
        Some(Err(e)) => Err(anyhow::anyhow!("connection failed: {e}")),
    }
}

/// Starts the host server: binds the listener, logs the address, and drives
/// the capture → encode → transport pipeline until shutdown.
///
/// # Errors
///
/// Returns an error if the QUIC handshake or streaming pipeline fails.
pub async fn serve(
    listener: QuicListener,
    config: HostConfig,
    shutdown: tokio::sync::oneshot::Receiver<()>,
) -> Result<()> {
    serve_with_handler(listener, shutdown, |transport, shutdown| {
        stream(transport, config, shutdown)
    })
    .await
}

// ── Streaming pipeline ────────────────────────────────────────────────────────

/// Drives the capture → encode → send loop on a connected transport.
///
/// The actual implementation lives behind `#[cfg(target_os = "windows")]`
/// because both screen capture (DXGI) and encoding (NVENC) are Windows-only.
/// On other platforms this function returns an error immediately so that the
/// macOS/Linux CI build stays green and coverage stays above the threshold.
#[cfg(target_os = "windows")]
async fn stream(
    mut transport: QuicVideoTransport,
    config: HostConfig,
    mut shutdown: tokio::sync::oneshot::Receiver<()>,
) -> Result<()> {
    use rayplay_video::{
        CaptureConfig, EncodedPacket, capture::CaptureError, create_capturer, create_encoder,
        frame::RawFrame,
    };

    let cap_config = CaptureConfig {
        target_fps: config.encoder_config.fps,
        acquire_timeout_ms: 100,
    };

    let capturer = create_capturer(cap_config).map_err(|e| anyhow::anyhow!("{e}"))?;
    let (cap_width, cap_height) = capturer.resolution();
    let enc_config = EncoderConfig::new(cap_width, cap_height, config.encoder_config.fps)
        .with_bitrate(config.encoder_config.bitrate);
    let mut encoder = create_encoder(enc_config).map_err(|e| anyhow::anyhow!("{e}"))?;

    // Channel from the blocking encode thread to the async send loop.
    let (packet_tx, mut packet_rx) = tokio::sync::mpsc::channel::<anyhow::Result<EncodedPacket>>(4);
    let session_start = std::time::Instant::now();

    // Capture and encode on a dedicated blocking thread — NVENC is synchronous.
    let _encode_handle = tokio::task::spawn_blocking(move || {
        loop {
            let frame = match capturer.capture_frame() {
                Ok(f) => f,
                Err(CaptureError::Timeout(_)) => continue,
                Err(e) => {
                    let _ = packet_tx.blocking_send(Err(anyhow::anyhow!("{e}")));
                    return;
                }
            };

            let ts = u64::try_from(session_start.elapsed().as_micros()).unwrap_or(u64::MAX);
            let raw = RawFrame::new(frame.data, frame.width, frame.height, frame.stride, ts);

            match encoder.encode(&raw) {
                Ok(Some(pkt)) => {
                    if packet_tx.blocking_send(Ok(pkt)).is_err() {
                        return; // receiver dropped — shutdown in progress
                    }
                }
                Ok(None) => {} // encoder buffering, wait for next frame
                Err(e) => {
                    let _ = packet_tx.blocking_send(Err(anyhow::anyhow!("{e}")));
                    return;
                }
            }
        }
    });

    // Forward encoded packets to the connected client until shutdown.
    loop {
        tokio::select! {
            _ = &mut shutdown => {
                tracing::info!("Shutdown signal received, stopping stream");
                break;
            }
            packet = packet_rx.recv() => {
                match packet {
                    Some(Ok(p)) => {
                        transport
                            .send_video(&p)
                            .await
                            .map_err(|e| anyhow::anyhow!("{e}"))?;
                    }
                    Some(Err(e)) => return Err(e),
                    None => break, // encode thread exited
                }
            }
        }
    }

    Ok(())
}

// The Windows version is `async`; keep the same signature here.
#[cfg(not(target_os = "windows"))]
#[allow(clippy::unused_async)]
async fn stream(
    _transport: QuicVideoTransport,
    _config: HostConfig,
    _shutdown: tokio::sync::oneshot::Receiver<()>,
) -> Result<()> {
    Err(anyhow::anyhow!(
        "screen capture and NVENC encoding are only supported on Windows"
    ))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use rayplay_network::QuicVideoTransport;

    use super::*;

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn default_args() -> HostArgs {
        HostArgs::parse_from(["rayhost"])
    }

    fn default_config() -> HostConfig {
        HostConfig::from_args(&default_args())
    }

    fn listen_loopback() -> (QuicListener, SocketAddr) {
        let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (listener, _cert) = QuicVideoTransport::listen(bind).unwrap();
        let addr = listener.local_addr().unwrap();
        (listener, addr)
    }

    // ── HostArgs defaults ─────────────────────────────────────────────────────

    #[test]
    fn test_host_args_default_port() {
        assert_eq!(default_args().port, 5000);
    }

    #[test]
    fn test_host_args_default_width() {
        assert_eq!(default_args().width, 1920);
    }

    #[test]
    fn test_host_args_default_height() {
        assert_eq!(default_args().height, 1080);
    }

    #[test]
    fn test_host_args_default_fps() {
        assert_eq!(default_args().fps, 60);
    }

    #[test]
    fn test_host_args_default_bitrate() {
        assert_eq!(default_args().bitrate, 0);
    }

    #[test]
    fn test_host_args_explicit_port() {
        let args = HostArgs::parse_from(["rayhost", "--port", "9000"]);
        assert_eq!(args.port, 9000);
    }

    #[test]
    fn test_host_args_explicit_resolution() {
        let args = HostArgs::parse_from(["rayhost", "--width", "3840", "--height", "2160"]);
        assert_eq!(args.width, 3840);
        assert_eq!(args.height, 2160);
    }

    #[test]
    fn test_host_args_explicit_fps() {
        let args = HostArgs::parse_from(["rayhost", "--fps", "120"]);
        assert_eq!(args.fps, 120);
    }

    #[test]
    fn test_host_args_explicit_bitrate() {
        let args = HostArgs::parse_from(["rayhost", "--bitrate", "20"]);
        assert_eq!(args.bitrate, 20);
    }

    // ── HostConfig::from_args ─────────────────────────────────────────────────

    #[test]
    fn test_host_config_bind_addr_uses_port() {
        let args = HostArgs::parse_from(["rayhost", "--port", "7777"]);
        assert_eq!(HostConfig::from_args(&args).bind_addr.port(), 7777);
    }

    #[test]
    fn test_host_config_bind_addr_is_unspecified() {
        assert!(default_config().bind_addr.ip().is_unspecified());
    }

    #[test]
    fn test_host_config_bitrate_auto_when_zero() {
        assert_eq!(default_config().encoder_config.bitrate, Bitrate::Auto);
    }

    #[test]
    fn test_host_config_bitrate_mbps_when_nonzero() {
        let args = HostArgs::parse_from(["rayhost", "--bitrate", "15"]);
        assert_eq!(
            HostConfig::from_args(&args).encoder_config.bitrate,
            Bitrate::Mbps(15)
        );
    }

    #[test]
    fn test_host_config_encoder_dimensions() {
        let args = HostArgs::parse_from(["rayhost", "--width", "2560", "--height", "1440"]);
        let cfg = HostConfig::from_args(&args);
        assert_eq!(cfg.encoder_config.width, 2560);
        assert_eq!(cfg.encoder_config.height, 1440);
    }

    #[test]
    fn test_host_config_encoder_fps() {
        let args = HostArgs::parse_from(["rayhost", "--fps", "30"]);
        assert_eq!(HostConfig::from_args(&args).encoder_config.fps, 30);
    }

    #[test]
    fn test_host_config_resolved_bitrate_auto_in_range() {
        let bps = default_config().encoder_config.resolved_bitrate();
        assert!((1_000_000..=100_000_000).contains(&bps));
    }

    #[test]
    fn test_host_config_resolved_bitrate_explicit() {
        let args = HostArgs::parse_from(["rayhost", "--bitrate", "10"]);
        assert_eq!(
            HostConfig::from_args(&args)
                .encoder_config
                .resolved_bitrate(),
            10_000_000
        );
    }

    // ── serve_with_handler ────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_serve_with_handler_shutdown_before_accept() {
        let (listener, _addr) = listen_loopback();
        let (tx, rx) = tokio::sync::oneshot::channel();
        tx.send(()).unwrap();
        let result = serve_with_handler(listener, rx, |_, _| async { anyhow::Ok(()) }).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_serve_with_handler_shutdown_while_waiting() {
        let (listener, _addr) = listen_loopback();
        let (tx, rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(serve_with_handler(listener, rx, |_, _| async {
            anyhow::Ok(())
        }));
        tokio::task::yield_now().await;
        tx.send(()).unwrap();
        assert!(task.await.unwrap().is_ok());
    }

    #[tokio::test]
    async fn test_serve_with_handler_client_connects_calls_on_connect() {
        let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (listener, cert_der) = QuicVideoTransport::listen(bind).unwrap();
        let addr = listener.local_addr().unwrap();
        let (_tx, rx) = tokio::sync::oneshot::channel::<()>(); // keep alive
        let task = tokio::spawn(serve_with_handler(
            listener,
            rx,
            |_transport, _shutdown| async { anyhow::Ok(()) },
        ));
        QuicVideoTransport::connect(addr, cert_der)
            .await
            .expect("connect");
        assert!(task.await.unwrap().is_ok());
    }

    #[tokio::test]
    async fn test_serve_with_handler_on_connect_error_propagates() {
        let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (listener, cert_der) = QuicVideoTransport::listen(bind).unwrap();
        let addr = listener.local_addr().unwrap();
        let (_tx, rx) = tokio::sync::oneshot::channel::<()>();
        let task = tokio::spawn(serve_with_handler(
            listener,
            rx,
            |_transport, _shutdown| async { Err(anyhow::anyhow!("handler error")) },
        ));
        QuicVideoTransport::connect(addr, cert_der)
            .await
            .expect("connect");
        let result = task.await.unwrap();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("handler error"));
    }

    #[tokio::test]
    async fn test_serve_with_handler_connection_failure_returns_error() {
        let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (listener, _correct_cert) = QuicVideoTransport::listen(bind).unwrap();
        let addr = listener.local_addr().unwrap();
        let (_, wrong_cert) = QuicVideoTransport::listen("127.0.0.1:0".parse().unwrap()).unwrap();
        let (_tx, rx) = tokio::sync::oneshot::channel::<()>();
        let task = tokio::spawn(serve_with_handler(listener, rx, |_, _| async {
            anyhow::Ok(())
        }));
        let _ = QuicVideoTransport::connect(addr, wrong_cert).await;
        let result = task.await.unwrap();
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("connection failed")
        );
    }

    // ── serve ─────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_serve_shuts_down_cleanly_before_accept() {
        let (listener, _addr) = listen_loopback();
        let (tx, rx) = tokio::sync::oneshot::channel();
        tx.send(()).unwrap();
        assert!(serve(listener, default_config(), rx).await.is_ok());
    }

    // ── stream (non-Windows stub) ─────────────────────────────────────────────

    #[cfg(not(target_os = "windows"))]
    #[tokio::test]
    async fn test_stream_returns_unsupported_error_on_non_windows() {
        let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (listener, cert_der) = QuicVideoTransport::listen(bind).unwrap();
        let addr = listener.local_addr().unwrap();
        let (_tx, rx) = tokio::sync::oneshot::channel::<()>();
        let task = tokio::spawn(serve(listener, default_config(), rx));
        QuicVideoTransport::connect(addr, cert_der)
            .await
            .expect("connect");
        let result = task.await.unwrap();
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("only supported on Windows")
        );
    }
}
