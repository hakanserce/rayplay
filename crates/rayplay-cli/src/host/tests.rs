use std::{
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Instant,
};

use rayplay_network::QuicVideoTransport;
use rayplay_video::{
    capture::{CaptureError, CapturedFrame, ScreenCapturer},
    encoder::{EncoderConfig, EncoderInput, VideoEncoder, VideoError},
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

fn empty_trust_db() -> std::sync::Arc<tokio::sync::Mutex<TrustDatabase>> {
    std::sync::Arc::new(tokio::sync::Mutex::new(TrustDatabase::new()))
}

/// Handler that succeeds immediately — used where the `on_connect` callback
/// must be callable and covered, and also passed (but never called) in
/// shutdown-before-connect tests so a single function body covers all cases.
async fn noop_handler(_: QuicVideoTransport, _: CancellationToken) -> Result<()> {
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
    fn capture_frame(&mut self) -> Result<CapturedFrame, CaptureError> {
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
    fn encode(&mut self, input: EncoderInput<'_>) -> Result<Option<EncodedPacket>, VideoError> {
        let timestamp_us = match &input {
            EncoderInput::Cpu(f) => f.timestamp_us,
            EncoderInput::GpuTexture { timestamp_us, .. } => *timestamp_us,
        };
        self.frame_count += 1;
        Ok(Some(EncodedPacket::new(
            vec![u8::try_from(self.frame_count).expect("frame count fits in u8")],
            self.frame_count == 1,
            timestamp_us,
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
    fn capture_frame(&mut self) -> Result<CapturedFrame, CaptureError> {
        Err(CaptureError::AcquireFailed("stub failure".to_owned()))
    }

    fn resolution(&self) -> (u32, u32) {
        (0, 0)
    }
}

/// Always returns an error when encoding.
struct FailingEncoder {
    config: EncoderConfig,
}

impl VideoEncoder for FailingEncoder {
    fn encode(&mut self, _input: EncoderInput<'_>) -> Result<Option<EncodedPacket>, VideoError> {
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

/// Returns `Ok(None)` on `encode()` — represents buffering behavior.
struct BufferingEncoder {
    config: EncoderConfig,
}

impl VideoEncoder for BufferingEncoder {
    fn encode(&mut self, _input: EncoderInput<'_>) -> Result<Option<EncodedPacket>, VideoError> {
        Ok(None)
    }

    fn flush(&mut self) -> Result<Vec<EncodedPacket>, VideoError> {
        Ok(vec![])
    }

    fn config(&self) -> &EncoderConfig {
        &self.config
    }
}

/// Returns `Timeout` for the first `n` calls, then `AcquireFailed`.
struct TimeoutThenFailCapturer {
    timeouts_remaining: AtomicUsize,
}

impl TimeoutThenFailCapturer {
    fn new(timeouts: usize) -> Self {
        Self {
            timeouts_remaining: AtomicUsize::new(timeouts),
        }
    }
}

impl ScreenCapturer for TimeoutThenFailCapturer {
    fn capture_frame(&mut self) -> Result<CapturedFrame, CaptureError> {
        if self.timeouts_remaining.load(Ordering::SeqCst) > 0 {
            self.timeouts_remaining.fetch_sub(1, Ordering::SeqCst);
            return Err(CaptureError::Timeout(std::time::Duration::from_millis(5)));
        }
        Err(CaptureError::AcquireFailed("stub failure".to_owned()))
    }

    fn resolution(&self) -> (u32, u32) {
        (0, 0)
    }
}

/// Panics inside `encode` — used to verify that an encode-thread panic is
/// surfaced as an error by `stream_with_pipeline` rather than silently lost.
struct PanickingEncoder {
    config: EncoderConfig,
}

impl VideoEncoder for PanickingEncoder {
    fn encode(&mut self, _input: EncoderInput<'_>) -> Result<Option<EncodedPacket>, VideoError> {
        panic!("deliberate encode-thread panic for testing");
    }

    fn flush(&mut self) -> Result<Vec<EncodedPacket>, VideoError> {
        Ok(vec![])
    }

    fn config(&self) -> &EncoderConfig {
        &self.config
    }
}

// ── Callback-based capturers (model SckCapturer pattern) ─────────────────

/// Models `SckCapturer` with proper cleanup: a background thread pushes frames
/// into a channel, and `Drop` stops the thread before releasing resources.
/// This is the *fixed* pattern — after the bug fix in #103.
struct CallbackCapturer {
    stop: Arc<AtomicBool>,
    rx: crossbeam_channel::Receiver<Vec<u8>>,
    thread: Option<std::thread::JoinHandle<()>>,
    width: u32,
    height: u32,
}

impl CallbackCapturer {
    fn new(width: u32, height: u32) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = crossbeam_channel::bounded(1);
        let stop_clone = stop.clone();
        let w = width;
        let h = height;
        let thread = std::thread::spawn(move || {
            while !stop_clone.load(Ordering::SeqCst) {
                let data = vec![0u8; (w * h * 4) as usize];
                match tx.try_send(data) {
                    Ok(()) | Err(crossbeam_channel::TrySendError::Full(_)) => {}
                    Err(crossbeam_channel::TrySendError::Disconnected(_)) => return,
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        });
        Self {
            stop,
            rx,
            thread: Some(thread),
            width,
            height,
        }
    }

    fn stop_flag(&self) -> Arc<AtomicBool> {
        self.stop.clone()
    }
}

impl Drop for CallbackCapturer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

impl ScreenCapturer for CallbackCapturer {
    fn capture_frame(&mut self) -> Result<CapturedFrame, CaptureError> {
        let data = self
            .rx
            .recv_timeout(std::time::Duration::from_millis(100))
            .map_err(|_| CaptureError::Timeout(std::time::Duration::from_millis(100)))?;
        Ok(CapturedFrame {
            width: self.width,
            height: self.height,
            stride: self.width * 4,
            data,
            timestamp: Instant::now(),
        })
    }

    fn resolution(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

/// Models `SckCapturer` WITHOUT cleanup — the pre-fix bug.
/// The background thread is NOT stopped on drop, demonstrating the leak
/// that leads to use-after-free / segfault in issue #103.
struct LeakyCallbackCapturer {
    #[allow(dead_code)] // intentionally unused — models the bug (no stop on drop)
    stop: Arc<AtomicBool>,
    rx: crossbeam_channel::Receiver<Vec<u8>>,
    width: u32,
    height: u32,
}

impl LeakyCallbackCapturer {
    fn new(width: u32, height: u32) -> (Self, Arc<AtomicBool>) {
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = crossbeam_channel::bounded(1);
        let stop_clone = stop.clone();
        let w = width;
        let h = height;
        std::thread::spawn(move || {
            while !stop_clone.load(Ordering::SeqCst) {
                let data = vec![0u8; (w * h * 4) as usize];
                match tx.try_send(data) {
                    Ok(()) | Err(crossbeam_channel::TrySendError::Full(_)) => {}
                    Err(crossbeam_channel::TrySendError::Disconnected(_)) => return,
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        });
        let flag = stop.clone();
        (
            Self {
                stop,
                rx,
                width,
                height,
            },
            flag,
        )
    }
}

impl ScreenCapturer for LeakyCallbackCapturer {
    fn capture_frame(&mut self) -> Result<CapturedFrame, CaptureError> {
        let data = self
            .rx
            .recv_timeout(std::time::Duration::from_millis(100))
            .map_err(|_| CaptureError::Timeout(std::time::Duration::from_millis(100)))?;
        Ok(CapturedFrame {
            width: self.width,
            height: self.height,
            stride: self.width * 4,
            data,
            timestamp: Instant::now(),
        })
    }

    fn resolution(&self) -> (u32, u32) {
        (self.width, self.height)
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
    assert!(e.encode(EncoderInput::Cpu(&raw)).unwrap().is_none());
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
    assert_eq!(e.config().width, cfg.width);
    assert_eq!(e.config().height, cfg.height);
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
fn test_host_args_default_software_is_false() {
    assert!(!default_args().software);
}

#[test]
fn test_host_args_software_flag() {
    let args = HostArgs::parse_from(["rayhost", "--software"]);
    assert!(args.software);
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
    use rayplay_video::encoder::Bitrate;
    assert_eq!(default_config().encoder_config.bitrate, Bitrate::Auto);
}

#[test]
fn test_host_config_bitrate_mbps_when_nonzero() {
    use rayplay_video::encoder::Bitrate;
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
fn test_host_config_pipeline_mode_auto_by_default() {
    use rayplay_video::PipelineMode;
    assert_eq!(default_config().pipeline_mode, PipelineMode::Auto);
}

#[test]
fn test_host_config_pipeline_mode_software_when_flag_set() {
    use rayplay_video::PipelineMode;
    let args = HostArgs::parse_from(["rayhost", "--software"]);
    assert_eq!(
        HostConfig::from_args(&args).pipeline_mode,
        PipelineMode::Software
    );
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

// ── drive_encode_loop ─────────────────────────────────────────────────────

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
    // Returns (doesn't panic)
}

/// Covers `CaptureError::Timeout` when the channel is still open: first call returns
/// `Timeout` (so the loop checks `is_closed()` — false — continues with another
/// open, so `is_closed()` is false → `continue`) then returns `AcquireFailed`
/// (so `drive_encode_loop` sends an error and exits deterministically).
#[test]
fn test_drive_encode_loop_timeout_continues_when_channel_open() {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<anyhow::Result<EncodedPacket>>(4);
    let capturer = Box::new(TimeoutThenFailCapturer::new(1));
    let encoder = Box::new(StubEncoder::new(EncoderConfig::new(2, 2, 60)));
    drive_encode_loop(capturer, encoder, tx, std::time::Instant::now());
    // Error message should come through
    let error = rx.try_recv().unwrap().unwrap_err();
    assert!(error.to_string().contains("stub failure"));
}

#[test]
fn test_drive_encode_loop_sends_capture_error_and_exits() {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<anyhow::Result<EncodedPacket>>(4);
    let capturer = Box::new(FailingCapturer);
    let encoder = Box::new(StubEncoder::new(EncoderConfig::new(2, 2, 60)));
    drive_encode_loop(capturer, encoder, tx, std::time::Instant::now());
    let error = rx.try_recv().unwrap().unwrap_err();
    assert!(error.to_string().contains("stub failure"));
    // Check that we actually got the error from capture process
    assert!(error.downcast_ref::<CaptureError>().is_some());
}

#[test]
fn test_drive_encode_loop_sends_encode_error_and_exits() {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<anyhow::Result<EncodedPacket>>(4);
    let capturer = Box::new(StubCapturer::new(1, 2, 2));
    let encoder = Box::new(FailingEncoder {
        config: EncoderConfig::new(2, 2, 60),
    });
    drive_encode_loop(capturer, encoder, tx, std::time::Instant::now());
    let error = rx.try_recv().unwrap().unwrap_err();
    assert!(error.to_string().contains("stub failure"));
    // Check that we actually got the error from encoder process
    assert!(error.downcast_ref::<VideoError>().is_some());
}

/// Covers the `blocking_send.is_err()` exit: receiver dropped before the
/// first successful send, so the encode path returns without panicking.
#[test]
fn test_drive_encode_loop_exits_when_blocking_send_fails() {
    let (tx, rx) = tokio::sync::mpsc::channel::<anyhow::Result<EncodedPacket>>(4);
    drop(rx);
    let capturer = Box::new(StubCapturer::new(1, 2, 2));
    let encoder = Box::new(StubEncoder::new(EncoderConfig::new(2, 2, 60)));
    drive_encode_loop(capturer, encoder, tx, std::time::Instant::now());
}

/// Covers `Ok(None)` buffering: encoder returns None for the first frame;
/// the loop continues and then exits on the next Timeout (channel closed).
#[test]
fn test_drive_encode_loop_ok_none_buffering_continues() {
    let (tx, rx) = tokio::sync::mpsc::channel::<anyhow::Result<EncodedPacket>>(4);
    drop(rx);
    let capturer = Box::new(StubCapturer::new(1, 2, 2));
    let encoder = Box::new(BufferingEncoder {
        config: EncoderConfig::new(2, 2, 60),
    });
    drive_encode_loop(capturer, encoder, tx, std::time::Instant::now());
}

// ── serve_with_handler ────────────────────────────────────────────────────

#[tokio::test]
async fn test_serve_with_handler_shutdown_before_accept() {
    let (listener, _addr) = listen_loopback();
    let token = CancellationToken::new();
    token.cancel();
    let result = serve_with_handler(listener, token, noop_handler).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_serve_with_handler_shutdown_while_waiting() {
    let (listener, _addr) = listen_loopback();
    let token = CancellationToken::new();
    let task = tokio::spawn(serve_with_handler(listener, token.clone(), noop_handler));
    tokio::task::yield_now().await;
    token.cancel();
    assert!(task.await.unwrap().is_ok());
}

#[tokio::test]
async fn test_serve_with_handler_client_connects_calls_on_connect() {
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (listener, cert_der) = QuicVideoTransport::listen(bind).unwrap();
    let addr = listener.local_addr().unwrap();
    let token = CancellationToken::new();
    let task = tokio::spawn(serve_with_handler(listener, token.clone(), noop_handler));
    QuicVideoTransport::connect(addr, cert_der)
        .await
        .expect("connect");
    tokio::task::yield_now().await;
    token.cancel();
    assert!(task.await.unwrap().is_ok());
}

#[tokio::test]
async fn test_serve_with_handler_on_connect_error_logged_and_loop_continues() {
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (listener, cert_der) = QuicVideoTransport::listen(bind).unwrap();
    let addr = listener.local_addr().unwrap();
    let token = CancellationToken::new();
    let token2 = token.clone();
    let task = tokio::spawn(serve_with_handler(
        listener,
        token,
        move |_transport, _child| {
            let t = token2.clone();
            async move {
                t.cancel();
                Err(anyhow::anyhow!("handler error"))
            }
        },
    ));
    QuicVideoTransport::connect(addr, cert_der)
        .await
        .expect("connect");
    assert!(task.await.unwrap().is_ok());
}

#[tokio::test]
async fn test_serve_with_handler_accept_error_retries() {
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (listener, _correct_cert) = QuicVideoTransport::listen(bind).unwrap();
    let addr = listener.local_addr().unwrap();
    let (_, wrong_cert) = QuicVideoTransport::listen("127.0.0.1:0".parse().unwrap()).unwrap();
    let token = CancellationToken::new();
    let token2 = token.clone();
    let task = tokio::spawn(async move {
        let _ = QuicVideoTransport::connect(addr, wrong_cert).await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        token2.cancel();
    });
    let result = serve_with_handler(listener, token, noop_handler).await;
    task.await.unwrap();
    assert!(result.is_ok());
}

// ── serve ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_serve_shuts_down_cleanly_before_accept() {
    let (listener, _addr) = listen_loopback();
    let token = CancellationToken::new();
    token.cancel();
    assert!(
        serve(listener, default_config(), empty_trust_db(), token)
            .await
            .is_ok()
    );
}

// ── run_send_loop ─────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_run_send_loop_none_breaks_and_returns_ok() {
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (listener, cert_der) = QuicVideoTransport::listen(bind).unwrap();
    let addr = listener.local_addr().unwrap();

    let (packet_tx, packet_rx) = tokio::sync::mpsc::channel::<anyhow::Result<EncodedPacket>>(4);
    drop(packet_tx);

    let token = CancellationToken::new();
    let server_task = tokio::spawn(async move {
        let transport = listener.accept().await.unwrap();
        run_send_loop(transport, packet_rx, token).await
    });

    QuicVideoTransport::connect(addr, cert_der).await.unwrap();
    assert!(server_task.await.unwrap().is_ok());
}

// ── Layer 1: QUIC transport only ─────────────────────────────────────────

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
        for i in 1..=u8::try_from(N).unwrap() {
            let pkt = EncodedPacket::new(vec![i], i == 1, 0, 16_667);
            transport.send_video(&pkt).await.unwrap();
            tokio::task::yield_now().await;
        }
    });

    let mut client = QuicVideoTransport::connect(addr, cert_der).await.unwrap();
    for i in 1..=u8::try_from(N).unwrap() {
        let pkt = client.recv_video().await.expect("receive packet");
        assert_eq!(pkt.data, vec![i]);
    }
    server_task.await.unwrap();
}

// ── Layer 2: drive_encode_loop → mpsc channel ────────────────────────────

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

    for i in 1..=u8::try_from(N).unwrap() {
        let pkt = rx
            .recv()
            .await
            .expect("channel open")
            .expect("no encode error");
        assert_eq!(pkt.data, vec![i]);
    }
}

// ── Layer 3: drive_encode_loop → channel → QUIC ─────────────────────────

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
    for i in 1..=u8::try_from(N).unwrap() {
        let pkt = client.recv_video().await.expect("receive packet");
        assert_eq!(pkt.data, vec![i]);
    }
    server_task.await.unwrap();
}

// ── Layer 4: stream_with_pipeline ────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_layer4_stream_with_pipeline_shutdown_before_first_frame() {
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (listener, cert_der) = QuicVideoTransport::listen(bind).unwrap();
    let addr = listener.local_addr().unwrap();

    let token = CancellationToken::new();
    let server_task = tokio::spawn({
        let token = token.clone();
        async move {
            let transport = listener.accept().await.unwrap();
            let capturer = Box::new(StubCapturer::new(0, 2, 2));
            let encoder = Box::new(StubEncoder::new(EncoderConfig::new(2, 2, 60)));
            stream_with_pipeline(transport, capturer, encoder, token).await
        }
    });

    QuicVideoTransport::connect(addr, cert_der).await.unwrap();
    tokio::task::yield_now().await;
    token.cancel();
    assert!(server_task.await.unwrap().is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_layer4_stream_with_pipeline_one_frame_received() {
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (listener, cert_der) = QuicVideoTransport::listen(bind).unwrap();
    let addr = listener.local_addr().unwrap();

    let token = CancellationToken::new();
    let server_task = tokio::spawn({
        let token = token.clone();
        async move {
            let transport = listener.accept().await.unwrap();
            let capturer = Box::new(StubCapturer::new(1, 2, 2));
            let encoder = Box::new(StubEncoder::new(EncoderConfig::new(2, 2, 60)));
            stream_with_pipeline(transport, capturer, encoder, token).await
        }
    });

    let mut client = QuicVideoTransport::connect(addr, cert_der).await.unwrap();
    let pkt = client.recv_video().await.expect("receive first packet");
    assert_eq!(pkt.data, vec![1u8]);

    token.cancel();
    assert!(server_task.await.unwrap().is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_layer4_stream_with_pipeline_three_frames_received_in_order() {
    const N: usize = 3;
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (listener, cert_der) = QuicVideoTransport::listen(bind).unwrap();
    let addr = listener.local_addr().unwrap();

    let token = CancellationToken::new();
    let server_task = tokio::spawn({
        let token = token.clone();
        async move {
            let transport = listener.accept().await.unwrap();
            let capturer = Box::new(StubCapturer::new(N, 2, 2));
            let encoder = Box::new(StubEncoder::new(EncoderConfig::new(2, 2, 60)));
            stream_with_pipeline(transport, capturer, encoder, token).await
        }
    });

    let mut client = QuicVideoTransport::connect(addr, cert_der).await.unwrap();
    for i in 1..=u8::try_from(N).unwrap() {
        let pkt = client.recv_video().await.expect("receive packet");
        assert_eq!(pkt.data, vec![i]);
    }

    token.cancel();
    assert!(server_task.await.unwrap().is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_layer4_stream_with_pipeline_capture_error_propagates() {
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (listener, cert_der) = QuicVideoTransport::listen(bind).unwrap();
    let addr = listener.local_addr().unwrap();

    let token = CancellationToken::new();
    let server_task = tokio::spawn(async move {
        let transport = listener.accept().await.unwrap();
        let capturer = Box::new(FailingCapturer);
        let encoder = Box::new(StubEncoder::new(EncoderConfig::new(2, 2, 60)));
        stream_with_pipeline(transport, capturer, encoder, token).await
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

    let token = CancellationToken::new();
    let server_task = tokio::spawn(async move {
        let transport = listener.accept().await.unwrap();
        let capturer = Box::new(StubCapturer::new(1, 2, 2));
        let encoder = Box::new(FailingEncoder {
            config: EncoderConfig::new(2, 2, 60),
        });
        stream_with_pipeline(transport, capturer, encoder, token).await
    });

    QuicVideoTransport::connect(addr, cert_der).await.unwrap();
    let result = server_task.await.unwrap();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("stub failure"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_layer4_stream_with_pipeline_encode_thread_panic_propagates_as_error() {
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (listener, cert_der) = QuicVideoTransport::listen(bind).unwrap();
    let addr = listener.local_addr().unwrap();

    let token = CancellationToken::new();
    let server_task = tokio::spawn(async move {
        let transport = listener.accept().await.unwrap();
        let capturer = Box::new(StubCapturer::new(1, 2, 2));
        let encoder = Box::new(PanickingEncoder {
            config: EncoderConfig::new(2, 2, 60),
        });
        stream_with_pipeline(transport, capturer, encoder, token).await
    });

    QuicVideoTransport::connect(addr, cert_der).await.unwrap();
    let result = server_task.await.unwrap();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("panicked"));
}

// ── End-to-End integration tests ─────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_e2e_frames_flow_from_host_to_client() {
    const N: usize = 5;
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (listener, cert_der) = QuicVideoTransport::listen(bind).unwrap();
    let addr = listener.local_addr().unwrap();

    let token = CancellationToken::new();
    let host_task = tokio::spawn({
        let token = token.clone();
        async move {
            serve_with_handler(listener, token, |transport, child| async move {
                let capturer = Box::new(StubCapturer::new(N, 2, 2));
                let encoder = Box::new(StubEncoder::new(EncoderConfig::new(2, 2, 60)));
                stream_with_pipeline(transport, capturer, encoder, child).await
            })
            .await
        }
    });

    let mut client = QuicVideoTransport::connect(addr, cert_der).await.unwrap();
    for i in 1..=u8::try_from(N).unwrap() {
        let pkt = client.recv_video().await.expect("recv_video");
        assert_eq!(pkt.data, vec![i]);
    }

    token.cancel();
    host_task.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_e2e_host_accepts_second_client_after_first_disconnects() {
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (listener, cert_der) = QuicVideoTransport::listen(bind).unwrap();
    let addr = listener.local_addr().unwrap();

    let token = CancellationToken::new();
    let host_task = tokio::spawn({
        let token = token.clone();
        async move {
            serve_with_handler(listener, token, |transport, child| async move {
                let capturer = Box::new(StubCapturer::new(3, 2, 2));
                let encoder = Box::new(StubEncoder::new(EncoderConfig::new(2, 2, 60)));
                stream_with_pipeline(transport, capturer, encoder, child).await
            })
            .await
        }
    });

    {
        let mut c1 = QuicVideoTransport::connect(addr, cert_der.clone())
            .await
            .unwrap();
        let _pkt = c1.recv_video().await.expect("client 1 recv");
    }
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    {
        let mut c2 = QuicVideoTransport::connect(addr, cert_der).await.unwrap();
        let pkt = c2.recv_video().await.expect("client 2 recv");
        assert!(!pkt.data.is_empty());
    }

    token.cancel();
    host_task.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_e2e_shutdown_token_stops_both_sides_cleanly() {
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (listener, cert_der) = QuicVideoTransport::listen(bind).unwrap();
    let addr = listener.local_addr().unwrap();

    let token = CancellationToken::new();
    let host_task = tokio::spawn({
        let token = token.clone();
        async move {
            serve_with_handler(listener, token, |transport, child| async move {
                let capturer = Box::new(StubCapturer::new(100, 2, 2));
                let encoder = Box::new(StubEncoder::new(EncoderConfig::new(2, 2, 60)));
                stream_with_pipeline(transport, capturer, encoder, child).await
            })
            .await
        }
    });

    let mut client = QuicVideoTransport::connect(addr, cert_der).await.unwrap();
    let _pkt = client.recv_video().await.expect("recv");
    token.cancel();
    assert!(host_task.await.unwrap().is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_e2e_network_error_does_not_crash_client() {
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (listener, cert_der) = QuicVideoTransport::listen(bind).unwrap();
    let addr = listener.local_addr().unwrap();

    let server_task = tokio::spawn(async move { listener.accept().await.unwrap() });
    let mut client = QuicVideoTransport::connect(addr, cert_der).await.unwrap();
    let server = server_task.await.unwrap();

    drop(server);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let result = client.recv_video().await;
    assert!(result.is_err());
}

// ── Callback capturer tests (issue #103) ─────────────────────────────────

#[test]
fn test_callback_capturer_resolution() {
    let c = CallbackCapturer::new(1920, 1080);
    assert_eq!(c.resolution(), (1920, 1080));
}

#[test]
fn test_callback_capturer_captures_frame() {
    let mut c = CallbackCapturer::new(2, 2);
    let frame = c.capture_frame().expect("should capture");
    assert_eq!(frame.width, 2);
    assert_eq!(frame.height, 2);
    assert_eq!(frame.data.len(), 16);
}

#[test]
fn test_leaky_capturer_resolution() {
    let (c, stop) = LeakyCallbackCapturer::new(640, 480);
    assert_eq!(c.resolution(), (640, 480));
    stop.store(true, Ordering::SeqCst);
}

#[test]
fn test_leaky_capturer_captures_frame() {
    let (mut c, stop) = LeakyCallbackCapturer::new(2, 2);
    let frame = c.capture_frame().expect("should capture");
    assert_eq!(frame.width, 2);
    assert_eq!(frame.data.len(), 16);
    stop.store(true, Ordering::SeqCst);
}

/// Demonstrates the bug from issue #103: dropping a capturer that has a
/// background callback thread but NO stop-on-drop leaves the thread running.
/// In the real `SckCapturer`, this means `ScreenCaptureKit` callbacks continue
/// firing on freed memory → segfault.
#[test]
fn test_leaky_capturer_background_thread_outlives_drop() {
    let (capturer, stop_flag) = LeakyCallbackCapturer::new(2, 2);

    // Drop the capturer without stopping the background thread.
    drop(capturer);
    std::thread::sleep(std::time::Duration::from_millis(50));

    // The background thread is STILL running — nobody stopped it.
    // In the real SckCapturer, this is the SCK dispatch queue callback
    // that causes a use-after-free segfault.
    assert!(
        !stop_flag.load(Ordering::SeqCst),
        "leaky capturer should NOT stop its background thread on drop"
    );

    // Clean up the test.
    stop_flag.store(true, Ordering::SeqCst);
}

/// Demonstrates the fix for issue #103: a capturer with proper Drop stops
/// the background thread, preventing use-after-free.
#[test]
fn test_callback_capturer_stops_background_thread_on_drop() {
    let capturer = CallbackCapturer::new(2, 2);
    let stop_flag = capturer.stop_flag();

    assert!(!stop_flag.load(Ordering::SeqCst), "not stopped yet");

    // Drop triggers the stop.
    drop(capturer);

    assert!(
        stop_flag.load(Ordering::SeqCst),
        "Drop should stop the background thread"
    );
}

/// Full reconnection scenario from issue #103: host streams to client 1,
/// client 1 disconnects, host accepts client 2 and streams successfully.
/// Uses `CallbackCapturer` (with proper Drop) to model `SckCapturer`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_e2e_callback_capturer_reconnect_after_disconnect() {
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (listener, cert_der) = QuicVideoTransport::listen(bind).unwrap();
    let addr = listener.local_addr().unwrap();

    let token = CancellationToken::new();
    let host_task = tokio::spawn({
        let token = token.clone();
        async move {
            serve_with_handler(listener, token, |transport, child| async move {
                let capturer = Box::new(CallbackCapturer::new(2, 2));
                let encoder = Box::new(StubEncoder::new(EncoderConfig::new(2, 2, 60)));
                stream_with_pipeline(transport, capturer, encoder, child).await
            })
            .await
        }
    });

    // Client 1: connect, receive one frame, disconnect.
    {
        let mut c1 = QuicVideoTransport::connect(addr, cert_der.clone())
            .await
            .unwrap();
        let _pkt = c1.recv_video().await.expect("client 1 recv");
    }
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Client 2: connect and receive one frame — proves host survived the
    // first disconnect without crashing (the #103 segfault).
    {
        let mut c2 = QuicVideoTransport::connect(addr, cert_der).await.unwrap();
        let pkt = c2.recv_video().await.expect("client 2 recv");
        assert!(!pkt.data.is_empty());
    }

    token.cancel();
    host_task.await.unwrap().unwrap();
}
