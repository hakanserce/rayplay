//! `RayHost` server — CLI configuration and connection-accept loop (UC-006).

use std::{future::Future, net::SocketAddr};

use anyhow::Result;
use clap::Parser;
use rayplay_network::{QuicListener, QuicVideoTransport};
use rayplay_video::encoder::{Bitrate, EncoderConfig};
#[cfg(any(target_os = "windows", test))]
use rayplay_video::{
    EncodedPacket,
    capture::{CaptureError, ScreenCapturer},
    encoder::VideoEncoder,
    frame::RawFrame,
};

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

/// Runs the capture-and-encode loop on the calling thread (intended for use
/// inside `spawn_blocking`).
///
/// Reads frames from `capturer`, encodes them with `encoder`, and sends the
/// resulting [`EncodedPacket`]s over `packet_tx`.  The loop exits when:
///
/// - `packet_tx` is closed (receiver dropped — stream is shutting down),
/// - a capture or encode error occurs (the error is forwarded via `packet_tx`),
/// - or a send on `packet_tx` fails because the receiver was already dropped.
#[cfg(any(target_os = "windows", test))]
pub(crate) fn drive_encode_loop(
    capturer: Box<dyn ScreenCapturer>,
    mut encoder: Box<dyn VideoEncoder>,
    packet_tx: tokio::sync::mpsc::Sender<anyhow::Result<EncodedPacket>>,
    session_start: std::time::Instant,
) {
    loop {
        let frame = match capturer.capture_frame() {
            Ok(f) => f,
            Err(CaptureError::Timeout(_)) => {
                if packet_tx.is_closed() {
                    return;
                }
                continue;
            }
            Err(e) => {
                let _ = packet_tx.blocking_send(Err(anyhow::Error::from(e)));
                return;
            }
        };

        let ts = u64::try_from(session_start.elapsed().as_micros()).unwrap_or(u64::MAX);
        let raw = RawFrame::new(frame.data, frame.width, frame.height, frame.stride, ts);

        match encoder.encode(&raw) {
            Ok(Some(pkt)) => {
                if packet_tx.blocking_send(Ok(pkt)).is_err() {
                    tracing::debug!("encode channel closed, stream is shutting down");
                    return;
                }
            }
            Ok(None) => {} // encoder buffering, wait for next frame
            Err(e) => {
                let _ = packet_tx.blocking_send(Err(anyhow::Error::from(e)));
                return;
            }
        }
    }
}

/// Forwards [`EncodedPacket`]s from `packet_rx` to `transport` until
/// `shutdown` fires or the channel is drained.
///
/// # Errors
///
/// Returns an error if `transport.send_video` fails or if the encode thread
/// signals an error via `packet_rx`.
#[cfg(any(target_os = "windows", test))]
async fn run_send_loop(
    mut transport: QuicVideoTransport,
    mut packet_rx: tokio::sync::mpsc::Receiver<anyhow::Result<EncodedPacket>>,
    mut shutdown: tokio::sync::oneshot::Receiver<()>,
) -> Result<()> {
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
                            .map_err(anyhow::Error::from)?;
                        // Yield after each send so the tokio I/O driver can
                        // transmit the queued datagram before the next packet.
                        tokio::task::yield_now().await;
                    }
                    Some(Err(e)) => return Err(e),
                    None => break, // encode thread exited cleanly
                }
            }
        }
    }
    Ok(())
}

/// Drives the encode-then-send loop given pre-built pipeline components.
///
/// Runs [`drive_encode_loop`] on a dedicated blocking thread (NVENC is
/// synchronous) and forwards packets to the connected client via
/// [`run_send_loop`] until `shutdown` fires.
///
/// # Errors
///
/// Returns an error if capture, encoding, or network transmission fails.
#[cfg(any(target_os = "windows", test))]
pub(crate) async fn stream_with_pipeline(
    transport: QuicVideoTransport,
    capturer: Box<dyn ScreenCapturer>,
    encoder: Box<dyn VideoEncoder>,
    shutdown: tokio::sync::oneshot::Receiver<()>,
) -> Result<()> {
    const ENCODE_CHANNEL_CAPACITY: usize = 4;

    let (packet_tx, packet_rx) =
        tokio::sync::mpsc::channel::<anyhow::Result<EncodedPacket>>(ENCODE_CHANNEL_CAPACITY);
    let session_start = std::time::Instant::now();

    let encode_handle = tokio::task::spawn_blocking(move || {
        drive_encode_loop(capturer, encoder, packet_tx, session_start);
    });

    let result = run_send_loop(transport, packet_rx, shutdown).await;
    // packet_rx is dropped above → packet_tx.is_closed() becomes true on the
    // next Timeout cycle and drive_encode_loop returns.  Awaiting the handle
    // here surfaces any encode-thread panic as an explicit error rather than
    // letting it go unobserved.
    encode_handle
        .await
        .map_err(|e| anyhow::anyhow!("encode thread panicked: {e}"))?;
    result
}

/// Resolves platform-specific capture and encoder then calls
/// [`stream_with_pipeline`].
#[cfg(target_os = "windows")]
async fn stream(
    transport: QuicVideoTransport,
    config: HostConfig,
    shutdown: tokio::sync::oneshot::Receiver<()>,
) -> Result<()> {
    use rayplay_video::{CaptureConfig, create_capturer, create_encoder};

    let cap_config = CaptureConfig {
        target_fps: config.encoder_config.fps,
        acquire_timeout_ms: 100,
    };

    let capturer = create_capturer(cap_config).map_err(anyhow::Error::from)?;
    let (cap_width, cap_height) = capturer.resolution();
    let enc_config = EncoderConfig::new(cap_width, cap_height, config.encoder_config.fps)
        .with_bitrate(config.encoder_config.bitrate);
    let encoder = create_encoder(enc_config).map_err(anyhow::Error::from)?;

    stream_with_pipeline(transport, capturer, encoder, shutdown).await
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
    use std::{
        net::SocketAddr,
        sync::atomic::{AtomicUsize, Ordering},
        time::Instant,
    };

    use rayplay_network::QuicVideoTransport;
    use rayplay_video::{
        capture::{CaptureError, CapturedFrame, ScreenCapturer},
        encoder::{EncoderConfig, VideoEncoder, VideoError},
        frame::RawFrame,
        packet::EncodedPacket,
    };

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

    /// Handler that succeeds immediately — used where the on_connect callback
    /// must be callable and covered, and also passed (but never called) in
    /// shutdown-before-connect tests so a single function body covers all cases.
    async fn noop_handler(
        _: QuicVideoTransport,
        _: tokio::sync::oneshot::Receiver<()>,
    ) -> Result<()> {
        Ok(())
    }

    // ── Stub pipeline components ──────────────────────────────────────────────

    /// Produces `n` frames then returns [`CaptureError::Timeout`] indefinitely.
    struct StubCapturer {
        frames_remaining: AtomicUsize,
        width: u32,
        height: u32,
    }

    impl StubCapturer {
        fn new(n: usize, width: u32, height: u32) -> Self {
            Self {
                frames_remaining: AtomicUsize::new(n),
                width,
                height,
            }
        }
    }

    impl ScreenCapturer for StubCapturer {
        fn capture_frame(&self) -> Result<CapturedFrame, CaptureError> {
            if self.frames_remaining.load(Ordering::SeqCst) == 0 {
                std::thread::sleep(std::time::Duration::from_millis(5));
                return Err(CaptureError::Timeout(std::time::Duration::from_millis(5)));
            }
            // Brief pause so the async send loop's select! can reach a pending
            // state between packets, giving tokio's I/O driver time to transmit
            // the QUIC datagrams before the next packet arrives.
            std::thread::sleep(std::time::Duration::from_millis(2));
            self.frames_remaining.fetch_sub(1, Ordering::SeqCst);
            Ok(CapturedFrame {
                width: self.width,
                height: self.height,
                stride: self.width * 4,
                data: vec![0u8; self.width as usize * self.height as usize * 4],
                timestamp: Instant::now(),
            })
        }

        fn resolution(&self) -> (u32, u32) {
            (self.width, self.height)
        }
    }

    /// Turns each frame into an `EncodedPacket` whose data is `[frame_number]`.
    struct StubEncoder {
        config: EncoderConfig,
        frame_count: usize,
    }

    impl StubEncoder {
        fn new(config: EncoderConfig) -> Self {
            Self {
                config,
                frame_count: 0,
            }
        }
    }

    impl VideoEncoder for StubEncoder {
        fn encode(&mut self, frame: &RawFrame) -> Result<Option<EncodedPacket>, VideoError> {
            self.frame_count += 1;
            Ok(Some(EncodedPacket::new(
                vec![u8::try_from(self.frame_count).expect("frame count fits in u8")],
                self.frame_count == 1,
                frame.timestamp_us,
                16_667,
            )))
        }

        fn flush(&mut self) -> Result<Vec<EncodedPacket>, VideoError> {
            Ok(vec![])
        }

        fn config(&self) -> &EncoderConfig {
            &self.config
        }
    }

    /// Immediately returns `CaptureError::AcquireFailed` on every call.
    struct FailingCapturer;

    impl ScreenCapturer for FailingCapturer {
        fn capture_frame(&self) -> Result<CapturedFrame, CaptureError> {
            Err(CaptureError::AcquireFailed("stub failure".to_owned()))
        }

        fn resolution(&self) -> (u32, u32) {
            (0, 0)
        }
    }

    /// Immediately returns `VideoError::EncodingFailed` from `encode`.
    struct FailingEncoder {
        config: EncoderConfig,
    }

    impl VideoEncoder for FailingEncoder {
        fn encode(&mut self, _frame: &RawFrame) -> Result<Option<EncodedPacket>, VideoError> {
            Err(VideoError::EncodingFailed {
                reason: "stub failure".to_owned(),
            })
        }

        fn flush(&mut self) -> Result<Vec<EncodedPacket>, VideoError> {
            Ok(vec![])
        }

        fn config(&self) -> &EncoderConfig {
            &self.config
        }
    }

    /// Returns [`CaptureError::Timeout`] `n` times (without sleeping), then
    /// returns [`CaptureError::AcquireFailed`].  Used to cover the `continue`
    /// branch of the Timeout arm while keeping the channel receiver alive.
    struct TimeoutThenFailCapturer {
        timeouts_remaining: AtomicUsize,
    }

    impl TimeoutThenFailCapturer {
        fn new(n: usize) -> Self {
            Self {
                timeouts_remaining: AtomicUsize::new(n),
            }
        }
    }

    impl ScreenCapturer for TimeoutThenFailCapturer {
        fn capture_frame(&self) -> Result<CapturedFrame, CaptureError> {
            if self.timeouts_remaining.load(Ordering::SeqCst) > 0 {
                self.timeouts_remaining.fetch_sub(1, Ordering::SeqCst);
                return Err(CaptureError::Timeout(std::time::Duration::ZERO));
            }
            Err(CaptureError::AcquireFailed("end".to_owned()))
        }

        fn resolution(&self) -> (u32, u32) {
            (0, 0)
        }
    }

    /// Always returns `Ok(None)` — simulates an encoder that buffers frames
    /// without producing output.
    struct BufferingEncoder {
        config: EncoderConfig,
    }

    impl VideoEncoder for BufferingEncoder {
        fn encode(&mut self, _frame: &RawFrame) -> Result<Option<EncodedPacket>, VideoError> {
            Ok(None)
        }

        fn flush(&mut self) -> Result<Vec<EncodedPacket>, VideoError> {
            Ok(vec![])
        }

        fn config(&self) -> &EncoderConfig {
            &self.config
        }
    }

    // ── Stub method coverage ──────────────────────────────────────────────────

    #[test]
    fn test_stub_capturer_resolution_returns_configured_dimensions() {
        let c = StubCapturer::new(0, 1920, 1080);
        assert_eq!(c.resolution(), (1920, 1080));
    }

    #[test]
    fn test_stub_encoder_flush_returns_empty_vec() {
        let mut e = StubEncoder::new(EncoderConfig::new(2, 2, 60));
        assert!(e.flush().unwrap().is_empty());
    }

    #[test]
    fn test_stub_encoder_config_returns_encoder_config() {
        let cfg = EncoderConfig::new(640, 480, 30);
        let e = StubEncoder::new(cfg.clone());
        assert_eq!(e.config().width, cfg.width);
        assert_eq!(e.config().height, cfg.height);
        assert_eq!(e.config().fps, cfg.fps);
    }

    #[test]
    fn test_failing_capturer_resolution_returns_zero() {
        assert_eq!(FailingCapturer.resolution(), (0, 0));
    }

    #[test]
    fn test_failing_encoder_flush_returns_empty_vec() {
        let mut e = FailingEncoder {
            config: EncoderConfig::new(2, 2, 60),
        };
        assert!(e.flush().unwrap().is_empty());
    }

    #[test]
    fn test_failing_encoder_config_returns_configured_dimensions() {
        let cfg = EncoderConfig::new(320, 240, 30);
        let e = FailingEncoder {
            config: cfg.clone(),
        };
        assert_eq!(e.config().width, cfg.width);
    }

    #[test]
    fn test_buffering_encoder_encode_returns_none() {
        let mut e = BufferingEncoder {
            config: EncoderConfig::new(2, 2, 60),
        };
        let raw = RawFrame::new(vec![0u8; 16], 2, 2, 8, 0);
        assert!(e.encode(&raw).unwrap().is_none());
    }

    #[test]
    fn test_buffering_encoder_flush_returns_empty_vec() {
        let mut e = BufferingEncoder {
            config: EncoderConfig::new(2, 2, 60),
        };
        assert!(e.flush().unwrap().is_empty());
    }

    #[test]
    fn test_buffering_encoder_config_returns_encoder_config() {
        let cfg = EncoderConfig::new(160, 90, 24);
        let e = BufferingEncoder {
            config: cfg.clone(),
        };
        assert_eq!(e.config().fps, cfg.fps);
    }

    /// Panics inside `encode` — used to verify that an encode-thread panic is
    /// surfaced as an error by `stream_with_pipeline` rather than silently lost.
    struct PanickingEncoder {
        config: EncoderConfig,
    }

    impl VideoEncoder for PanickingEncoder {
        fn encode(&mut self, _frame: &RawFrame) -> Result<Option<EncodedPacket>, VideoError> {
            panic!("deliberate encode-thread panic for testing");
        }

        fn flush(&mut self) -> Result<Vec<EncodedPacket>, VideoError> {
            Ok(vec![])
        }

        fn config(&self) -> &EncoderConfig {
            &self.config
        }
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
    fn test_host_args_short_port_flag() {
        let args = HostArgs::parse_from(["rayhost", "-p", "8080"]);
        assert_eq!(args.port, 8080);
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
        let result = serve_with_handler(listener, rx, noop_handler).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_serve_with_handler_shutdown_while_waiting() {
        let (listener, _addr) = listen_loopback();
        let (tx, rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(serve_with_handler(listener, rx, noop_handler));
        tokio::task::yield_now().await;
        tx.send(()).unwrap();
        assert!(task.await.unwrap().is_ok());
    }

    #[tokio::test]
    async fn test_serve_with_handler_client_connects_calls_on_connect() {
        let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (listener, cert_der) = QuicVideoTransport::listen(bind).unwrap();
        let addr = listener.local_addr().unwrap();
        let (_tx, rx) = tokio::sync::oneshot::channel::<()>();
        let task = tokio::spawn(serve_with_handler(listener, rx, noop_handler));
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
        let task = tokio::spawn(serve_with_handler(listener, rx, noop_handler));
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

    // ── drive_encode_loop — direct unit tests ─────────────────────────────────
    //
    // Called synchronously (no spawn_blocking) so every branch is directly
    // instrumented by llvm-cov without any blocking-thread coverage gaps.

    #[test]
    fn test_timeout_then_fail_capturer_resolution_returns_zero() {
        assert_eq!(TimeoutThenFailCapturer::new(0).resolution(), (0, 0));
    }

    #[test]
    fn test_drive_encode_loop_exits_on_closed_channel_via_timeout() {
        let (tx, rx) = tokio::sync::mpsc::channel::<anyhow::Result<EncodedPacket>>(4);
        drop(rx); // is_closed() returns true after the first Timeout
        let capturer = Box::new(StubCapturer::new(0, 2, 2));
        let encoder = Box::new(StubEncoder::new(EncoderConfig::new(2, 2, 60)));
        drive_encode_loop(capturer, encoder, tx, std::time::Instant::now());
        // Returns promptly; no panic.
    }

    /// Covers the `continue` path in the Timeout arm (is_closed() == false).
    ///
    /// `TimeoutThenFailCapturer` returns one Timeout (with the channel still
    /// open, so is_closed() is false → `continue`) then returns `AcquireFailed`
    /// (so drive_encode_loop sends an error and exits deterministically).
    #[test]
    fn test_drive_encode_loop_timeout_continues_when_channel_open() {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<anyhow::Result<EncodedPacket>>(4);
        let capturer = Box::new(TimeoutThenFailCapturer::new(1));
        let encoder = Box::new(StubEncoder::new(EncoderConfig::new(2, 2, 60)));
        drive_encode_loop(capturer, encoder, tx, std::time::Instant::now());
        let result = rx.try_recv().expect("error was sent");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("end"));
    }

    #[test]
    fn test_drive_encode_loop_sends_capture_error_and_exits() {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<anyhow::Result<EncodedPacket>>(4);
        let capturer = Box::new(FailingCapturer);
        let encoder = Box::new(StubEncoder::new(EncoderConfig::new(2, 2, 60)));
        drive_encode_loop(capturer, encoder, tx, std::time::Instant::now());
        let result = rx
            .try_recv()
            .expect("error was sent before function returned");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("stub failure"));
    }

    #[test]
    fn test_drive_encode_loop_sends_encode_error_and_exits() {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<anyhow::Result<EncodedPacket>>(4);
        let capturer = Box::new(StubCapturer::new(1, 2, 2));
        let encoder = Box::new(FailingEncoder {
            config: EncoderConfig::new(2, 2, 60),
        });
        drive_encode_loop(capturer, encoder, tx, std::time::Instant::now());
        let result = rx
            .try_recv()
            .expect("error was sent before function returned");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("stub failure"));
    }

    /// Covers the `blocking_send.is_err()` exit: receiver dropped before the
    /// first successful send, so the encode path returns without panicking.
    #[test]
    fn test_drive_encode_loop_exits_when_blocking_send_fails() {
        let (tx, rx) = tokio::sync::mpsc::channel::<anyhow::Result<EncodedPacket>>(4);
        drop(rx);
        // StubCapturer(1) returns a real frame (not a Timeout), so the
        // is_closed() check in the Timeout branch is bypassed — we go straight
        // to encode→blocking_send, which then fails because rx was dropped.
        let capturer = Box::new(StubCapturer::new(1, 2, 2));
        let encoder = Box::new(StubEncoder::new(EncoderConfig::new(2, 2, 60)));
        drive_encode_loop(capturer, encoder, tx, std::time::Instant::now());
    }

    /// Covers `Ok(None)` buffering: encoder returns None for the first frame;
    /// the loop continues and then exits on the next Timeout (channel closed).
    #[test]
    fn test_drive_encode_loop_ok_none_buffering_continues() {
        let (tx, rx) = tokio::sync::mpsc::channel::<anyhow::Result<EncodedPacket>>(4);
        drop(rx); // is_closed() becomes true after Ok(None) → continue → Timeout
        let capturer = Box::new(StubCapturer::new(1, 2, 2));
        let encoder = Box::new(BufferingEncoder {
            config: EncoderConfig::new(2, 2, 60),
        });
        drive_encode_loop(capturer, encoder, tx, std::time::Instant::now());
    }

    // ── run_send_loop — direct unit tests ─────────────────────────────────────

    /// Covers the `None => break` arm: all senders dropped before the loop
    /// starts, so the first `packet_rx.recv()` returns `None` immediately.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_run_send_loop_none_breaks_and_returns_ok() {
        let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (listener, cert_der) = QuicVideoTransport::listen(bind).unwrap();
        let addr = listener.local_addr().unwrap();

        let (packet_tx, packet_rx) = tokio::sync::mpsc::channel::<anyhow::Result<EncodedPacket>>(4);
        drop(packet_tx); // recv() will return None immediately

        let (_shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let server_task = tokio::spawn(async move {
            let transport = listener.accept().await.unwrap();
            run_send_loop(transport, packet_rx, shutdown_rx).await
        });

        QuicVideoTransport::connect(addr, cert_der).await.unwrap();
        assert!(server_task.await.unwrap().is_ok());
    }

    // ── Layer 1: QUIC transport only (server → client direction) ─────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_layer1_quic_server_sends_one_packet_to_client() {
        let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (listener, cert_der) = QuicVideoTransport::listen(bind).unwrap();
        let addr = listener.local_addr().unwrap();

        let server_task = tokio::spawn(async move {
            let mut transport = listener.accept().await.unwrap();
            let pkt = EncodedPacket::new(vec![42u8], true, 0, 16_667);
            transport.send_video(&pkt).await.unwrap();
            tokio::task::yield_now().await;
        });

        let mut client = QuicVideoTransport::connect(addr, cert_der).await.unwrap();
        let pkt = client.recv_video().await.expect("receive packet");
        assert_eq!(pkt.data, vec![42u8]);
        server_task.await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_layer1_quic_server_sends_three_packets_to_client() {
        const N: usize = 3;
        let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (listener, cert_der) = QuicVideoTransport::listen(bind).unwrap();
        let addr = listener.local_addr().unwrap();

        let server_task = tokio::spawn(async move {
            let mut transport = listener.accept().await.unwrap();
            for i in 1u8..=N as u8 {
                let pkt = EncodedPacket::new(vec![i], i == 1, 0, 16_667);
                transport.send_video(&pkt).await.unwrap();
                tokio::task::yield_now().await;
            }
        });

        let mut client = QuicVideoTransport::connect(addr, cert_der).await.unwrap();
        for i in 1u8..=N as u8 {
            let pkt = client.recv_video().await.expect("receive packet");
            assert_eq!(pkt.data, vec![i]);
        }
        server_task.await.unwrap();
    }

    // ── Layer 2: drive_encode_loop → mpsc channel (no QUIC) ──────────────────

    #[tokio::test]
    async fn test_layer2_encode_thread_delivers_n_packets_to_channel() {
        const N: usize = 3;
        let (tx, mut rx) = tokio::sync::mpsc::channel::<anyhow::Result<EncodedPacket>>(4);
        let session_start = std::time::Instant::now();

        let capturer = Box::new(StubCapturer::new(N, 2, 2));
        let encoder = Box::new(StubEncoder::new(EncoderConfig::new(2, 2, 60)));

        tokio::task::spawn_blocking(move || {
            drive_encode_loop(capturer, encoder, tx, session_start);
        });

        for i in 1u8..=N as u8 {
            let pkt = rx
                .recv()
                .await
                .expect("channel open")
                .expect("no encode error");
            assert_eq!(pkt.data, vec![i]);
        }
        // rx drops here → encode thread exits via is_closed() on next Timeout.
    }

    // ── Layer 3: drive_encode_loop → channel → QUIC (manual wiring) ──────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_layer3_encode_to_quic_delivers_n_packets() {
        const N: usize = 3;
        let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (listener, cert_der) = QuicVideoTransport::listen(bind).unwrap();
        let addr = listener.local_addr().unwrap();

        let server_task = tokio::spawn(async move {
            let mut transport = listener.accept().await.unwrap();
            let (tx, mut rx) = tokio::sync::mpsc::channel::<anyhow::Result<EncodedPacket>>(4);
            let session_start = std::time::Instant::now();
            let capturer = Box::new(StubCapturer::new(N, 2, 2));
            let encoder = Box::new(StubEncoder::new(EncoderConfig::new(2, 2, 60)));

            tokio::task::spawn_blocking(move || {
                drive_encode_loop(capturer, encoder, tx, session_start);
            });

            for _ in 0..N {
                let pkt = rx.recv().await.unwrap().unwrap();
                transport.send_video(&pkt).await.unwrap();
                tokio::task::yield_now().await;
            }
        });

        let mut client = QuicVideoTransport::connect(addr, cert_der).await.unwrap();
        for i in 1u8..=N as u8 {
            let pkt = client.recv_video().await.expect("receive packet");
            assert_eq!(pkt.data, vec![i]);
        }
        server_task.await.unwrap();
    }

    // ── Layer 4: stream_with_pipeline (full abstraction + shutdown) ───────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_layer4_stream_with_pipeline_shutdown_before_first_frame() {
        let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (listener, cert_der) = QuicVideoTransport::listen(bind).unwrap();
        let addr = listener.local_addr().unwrap();

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let server_task = tokio::spawn(async move {
            let transport = listener.accept().await.unwrap();
            let capturer = Box::new(StubCapturer::new(0, 2, 2));
            let encoder = Box::new(StubEncoder::new(EncoderConfig::new(2, 2, 60)));
            stream_with_pipeline(transport, capturer, encoder, shutdown_rx).await
        });

        QuicVideoTransport::connect(addr, cert_der).await.unwrap();
        tokio::task::yield_now().await;
        shutdown_tx.send(()).unwrap();
        assert!(server_task.await.unwrap().is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_layer4_stream_with_pipeline_one_frame_received() {
        let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (listener, cert_der) = QuicVideoTransport::listen(bind).unwrap();
        let addr = listener.local_addr().unwrap();

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let server_task = tokio::spawn(async move {
            let transport = listener.accept().await.unwrap();
            let capturer = Box::new(StubCapturer::new(1, 2, 2));
            let encoder = Box::new(StubEncoder::new(EncoderConfig::new(2, 2, 60)));
            stream_with_pipeline(transport, capturer, encoder, shutdown_rx).await
        });

        let mut client = QuicVideoTransport::connect(addr, cert_der).await.unwrap();
        let pkt = client.recv_video().await.expect("receive first packet");
        assert_eq!(pkt.data, vec![1u8]);

        shutdown_tx.send(()).unwrap();
        assert!(server_task.await.unwrap().is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_layer4_stream_with_pipeline_three_frames_received_in_order() {
        const N: usize = 3;
        let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (listener, cert_der) = QuicVideoTransport::listen(bind).unwrap();
        let addr = listener.local_addr().unwrap();

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let server_task = tokio::spawn(async move {
            let transport = listener.accept().await.unwrap();
            let capturer = Box::new(StubCapturer::new(N, 2, 2));
            let encoder = Box::new(StubEncoder::new(EncoderConfig::new(2, 2, 60)));
            stream_with_pipeline(transport, capturer, encoder, shutdown_rx).await
        });

        let mut client = QuicVideoTransport::connect(addr, cert_der).await.unwrap();
        for i in 1u8..=N as u8 {
            let pkt = client.recv_video().await.expect("receive packet");
            assert_eq!(pkt.data, vec![i]);
        }

        shutdown_tx.send(()).unwrap();
        assert!(server_task.await.unwrap().is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_layer4_stream_with_pipeline_capture_error_propagates() {
        let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (listener, cert_der) = QuicVideoTransport::listen(bind).unwrap();
        let addr = listener.local_addr().unwrap();

        let (_tx, rx) = tokio::sync::oneshot::channel::<()>();
        let server_task = tokio::spawn(async move {
            let transport = listener.accept().await.unwrap();
            let capturer = Box::new(FailingCapturer);
            let encoder = Box::new(StubEncoder::new(EncoderConfig::new(2, 2, 60)));
            stream_with_pipeline(transport, capturer, encoder, rx).await
        });

        QuicVideoTransport::connect(addr, cert_der).await.unwrap();
        let result = server_task.await.unwrap();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("stub failure"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_layer4_stream_with_pipeline_encode_error_propagates() {
        let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (listener, cert_der) = QuicVideoTransport::listen(bind).unwrap();
        let addr = listener.local_addr().unwrap();

        let (_tx, rx) = tokio::sync::oneshot::channel::<()>();
        let server_task = tokio::spawn(async move {
            let transport = listener.accept().await.unwrap();
            let capturer = Box::new(StubCapturer::new(1, 2, 2));
            let encoder = Box::new(FailingEncoder {
                config: EncoderConfig::new(2, 2, 60),
            });
            stream_with_pipeline(transport, capturer, encoder, rx).await
        });

        QuicVideoTransport::connect(addr, cert_der).await.unwrap();
        let result = server_task.await.unwrap();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("stub failure"));
    }

    /// Covers the `encode_handle.await` panic path in `stream_with_pipeline`.
    ///
    /// When the encode thread panics, `packet_tx` is dropped (unwind drops it),
    /// so `packet_rx.recv()` returns `None`, `run_send_loop` exits cleanly, and
    /// the awaited `JoinHandle` returns a `JoinError` which we map to an error.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_layer4_stream_with_pipeline_encode_thread_panic_propagates_as_error() {
        let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (listener, cert_der) = QuicVideoTransport::listen(bind).unwrap();
        let addr = listener.local_addr().unwrap();

        let (_tx, rx) = tokio::sync::oneshot::channel::<()>();
        let server_task = tokio::spawn(async move {
            let transport = listener.accept().await.unwrap();
            let capturer = Box::new(StubCapturer::new(1, 2, 2));
            let encoder = Box::new(PanickingEncoder {
                config: EncoderConfig::new(2, 2, 60),
            });
            stream_with_pipeline(transport, capturer, encoder, rx).await
        });

        QuicVideoTransport::connect(addr, cert_der).await.unwrap();
        let result = server_task.await.unwrap();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("panicked"));
    }

    #[test]
    fn test_panicking_encoder_flush_returns_empty_vec() {
        let mut e = PanickingEncoder {
            config: EncoderConfig::new(2, 2, 60),
        };
        assert!(e.flush().unwrap().is_empty());
    }

    #[test]
    fn test_panicking_encoder_config_returns_encoder_config() {
        let cfg = EncoderConfig::new(2, 2, 60);
        let e = PanickingEncoder {
            config: cfg.clone(),
        };
        assert_eq!(e.config().width, cfg.width);
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
