//! `RayHost` server — CLI configuration and connection-accept loop (UC-006, UC-008).

use std::{future::Future, net::SocketAddr};

use anyhow::Result;
use clap::Parser;
use rayplay_core::pairing::TrustDatabase;
use rayplay_network::{QuicListener, QuicVideoTransport};
use rayplay_video::PipelineMode;
#[cfg(target_os = "windows")]
use rayplay_video::encoder::GpuTextureHandle;
use rayplay_video::encoder::{Bitrate, EncoderConfig};
use rayplay_video::{
    EncodedPacket,
    capture::{CaptureError, ScreenCapturer},
    encoder::{EncoderInput, VideoEncoder},
    frame::RawFrame,
};
use tokio_util::sync::CancellationToken;

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

    /// Force software pipeline — skip hardware acceleration even on supported platforms.
    #[arg(long)]
    pub software: bool,
}

/// Resolved server configuration derived from [`HostArgs`].
#[derive(Debug, Clone)]
pub struct HostConfig {
    /// Address the QUIC listener will bind to.
    pub bind_addr: SocketAddr,
    /// Video encoder settings derived from the CLI arguments.
    pub encoder_config: EncoderConfig,
    /// Pipeline mode (auto or forced software).
    pub pipeline_mode: PipelineMode,
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
        let pipeline_mode = if args.software {
            PipelineMode::Software
        } else {
            PipelineMode::Auto
        };
        Self {
            bind_addr,
            encoder_config,
            pipeline_mode,
        }
    }
}

// ── Accept loop ───────────────────────────────────────────────────────────────

/// Accepts clients in a loop, calling `on_connect` for each connection, until
/// `token` is cancelled.
///
/// After a client disconnects (handler returns), the loop continues accepting
/// the next client.  Accept errors are logged and the loop continues.
///
/// # Errors
///
/// Propagates errors from `on_connect`.
pub(crate) async fn serve_with_handler<F, Fut>(
    listener: QuicListener,
    token: CancellationToken,
    on_connect: F,
) -> Result<()>
where
    F: Fn(QuicVideoTransport, CancellationToken) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    loop {
        let accept_result = tokio::select! {
            () = token.cancelled() => return Ok(()),
            result = listener.accept() => result,
        };

        match accept_result {
            Ok(transport) => {
                tracing::info!("Client connected");
                let child = token.child_token();
                if let Err(e) = on_connect(transport, child).await {
                    tracing::warn!(error = %e, "Client session ended with error");
                }
                tracing::info!("Client disconnected, waiting for next connection");
            }
            Err(e) => {
                if token.is_cancelled() {
                    return Ok(());
                }
                tracing::warn!(error = %e, "Accept failed, retrying");
            }
        }
    }
}

/// Starts the host server: binds the listener, logs the address, and drives
/// the capture → encode → transport pipeline until shutdown.
///
/// For each incoming connection, the host first attempts authentication:
/// 1. Try challenge-response auth (trusted client reconnection).
/// 2. If auth fails, generate a PIN and run SPAKE2 pairing.
/// 3. Only after successful auth/pairing does media streaming begin.
///
/// # Errors
///
/// Returns an error if the QUIC handshake or streaming pipeline fails.
pub async fn serve(
    listener: QuicListener,
    config: HostConfig,
    trust_db: std::sync::Arc<tokio::sync::Mutex<TrustDatabase>>,
    token: CancellationToken,
) -> Result<()> {
    serve_with_handler(listener, token, |transport, child| {
        let config = config.clone();
        let trust_db = trust_db.clone();
        async move {
            crate::host_pairing_glue::authenticate_and_stream(transport, config, trust_db, child)
                .await
        }
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
#[allow(clippy::needless_pass_by_value)] // takes ownership to drop sender on loop exit
pub(crate) fn drive_encode_loop(
    mut capturer: Box<dyn ScreenCapturer>,
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
        tracing::debug!(timestamp_us = ts, "frame_captured");
        let raw = RawFrame::new(frame.data, frame.width, frame.height, frame.stride, ts);

        match encoder.encode(EncoderInput::Cpu(&raw)) {
            Ok(Some(pkt)) => {
                tracing::debug!(timestamp_us = ts, size = pkt.data.len(), "frame_encoded");
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
async fn run_send_loop(
    mut transport: QuicVideoTransport,
    mut packet_rx: tokio::sync::mpsc::Receiver<anyhow::Result<EncodedPacket>>,
    token: CancellationToken,
) -> Result<()> {
    loop {
        tokio::select! {
            () = token.cancelled() => {
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
                        tracing::debug!(timestamp_us = p.timestamp_us, "packet_sent");
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
pub(crate) async fn stream_with_pipeline(
    transport: QuicVideoTransport,
    capturer: Box<dyn ScreenCapturer>,
    encoder: Box<dyn VideoEncoder>,
    token: CancellationToken,
) -> Result<()> {
    const ENCODE_CHANNEL_CAPACITY: usize = 4;

    let (packet_tx, packet_rx) =
        tokio::sync::mpsc::channel::<anyhow::Result<EncodedPacket>>(ENCODE_CHANNEL_CAPACITY);
    let session_start = std::time::Instant::now();

    let encode_handle = tokio::task::spawn_blocking(move || {
        drive_encode_loop(capturer, encoder, packet_tx, session_start);
    });

    let result = run_send_loop(transport, packet_rx, token).await;
    // packet_rx is dropped above → packet_tx.is_closed() becomes true on the
    // next Timeout cycle and drive_encode_loop returns.  Awaiting the handle
    // here surfaces any encode-thread panic as an explicit error rather than
    // letting it go unobserved.
    encode_handle
        .await
        .map_err(|e| anyhow::anyhow!("encode thread panicked: {e}"))?;
    result
}

/// Runs the zero-copy capture-and-encode loop on a dedicated blocking thread.
///
/// Acquires GPU textures from `capturer`, passes them directly to the encoder
/// as [`EncoderInput::GpuTexture`], and sends resulting packets over `packet_tx`.
/// The DXGI frame is released after encoding completes.
#[cfg(target_os = "windows")]
pub(crate) fn drive_zero_copy_encode_loop(
    capturer: impl rayplay_video::capture::ZeroCopyCapturer,
    mut encoder: impl rayplay_video::encoder::VideoEncoder,
    packet_tx: tokio::sync::mpsc::Sender<anyhow::Result<EncodedPacket>>,
    session_start: std::time::Instant,
) {
    use rayplay_video::capture::CaptureError;

    loop {
        let texture = match capturer.acquire_texture() {
            Ok(t) => t,
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
        tracing::debug!(timestamp_us = ts, "zero_copy_frame_captured");

        // RAII guard ensures `release_frame` is called even if `encode` panics.
        struct FrameGuard<'c, C: rayplay_video::capture::ZeroCopyCapturer>(&'c C);
        impl<C: rayplay_video::capture::ZeroCopyCapturer> Drop for FrameGuard<'_, C> {
            fn drop(&mut self) {
                self.0.release_frame();
            }
        }
        let _guard = FrameGuard(&capturer);

        let input = EncoderInput::GpuTexture {
            handle: GpuTextureHandle(texture.texture_ptr),
            width: texture.width,
            height: texture.height,
            timestamp_us: ts,
        };

        let result = encoder.encode(input);

        match result {
            Ok(Some(pkt)) => {
                tracing::debug!(timestamp_us = ts, size = pkt.data.len(), "frame_encoded");
                if packet_tx.blocking_send(Ok(pkt)).is_err() {
                    tracing::debug!("encode channel closed, stream is shutting down");
                    return;
                }
            }
            Ok(None) => {}
            Err(e) => {
                let _ = packet_tx.blocking_send(Err(anyhow::Error::from(e)));
                return;
            }
        }
    }
}

/// Drives the zero-copy encode-then-send loop with pre-built pipeline components.
///
/// Similar to [`stream_with_pipeline`] but uses [`drive_zero_copy_encode_loop`]
/// for the GPU-to-GPU path.
#[cfg(target_os = "windows")]
pub(crate) async fn stream_with_zero_copy_pipeline(
    transport: QuicVideoTransport,
    capturer: impl rayplay_video::capture::ZeroCopyCapturer + 'static,
    encoder: impl rayplay_video::encoder::VideoEncoder + 'static,
    token: CancellationToken,
) -> Result<()> {
    const ENCODE_CHANNEL_CAPACITY: usize = 4;

    let (packet_tx, packet_rx) =
        tokio::sync::mpsc::channel::<anyhow::Result<EncodedPacket>>(ENCODE_CHANNEL_CAPACITY);
    let session_start = std::time::Instant::now();

    let encode_handle = tokio::task::spawn_blocking(move || {
        drive_zero_copy_encode_loop(capturer, encoder, packet_tx, session_start);
    });

    let result = run_send_loop(transport, packet_rx, token).await;
    encode_handle
        .await
        .map_err(|e| anyhow::anyhow!("encode thread panicked: {e}"))?;
    result
}

/// Resolves platform-specific capture and encoder then calls
/// [`stream_with_zero_copy_pipeline`] for the zero-copy GPU path.
#[cfg(target_os = "windows")]
pub(crate) async fn stream(
    transport: QuicVideoTransport,
    config: HostConfig,
    token: CancellationToken,
) -> Result<()> {
    use std::sync::Arc;

    use rayplay_video::{
        CaptureConfig, SharedD3D11Device, capture::ZeroCopyCapturer, dxgi_capture::DxgiCapture,
        nvenc::NvencEncoder,
    };

    let device = Arc::new(SharedD3D11Device::new().map_err(anyhow::Error::from)?);

    let cap_config = CaptureConfig {
        target_fps: config.encoder_config.fps,
        acquire_timeout_ms: 100,
    };
    let capturer = DxgiCapture::new(cap_config, device.clone()).map_err(anyhow::Error::from)?;
    let (cap_width, cap_height) = <DxgiCapture as ZeroCopyCapturer>::resolution(&capturer);

    let enc_config = EncoderConfig::new(cap_width, cap_height, config.encoder_config.fps)
        .with_bitrate(config.encoder_config.bitrate);
    let encoder = NvencEncoder::new(enc_config).map_err(anyhow::Error::from)?;

    stream_with_zero_copy_pipeline(transport, capturer, encoder, token).await
}

/// Non-Windows streaming path — delegates to platform-specific modules.
///
/// On macOS, uses [`host_capture_macos`](crate::host_capture_macos) which
/// checks Screen Recording permission and captures via `ScreenCaptureKit`.
/// On other non-Windows platforms, uses the software fallback pipeline.
#[cfg(target_os = "macos")]
pub(crate) async fn stream(
    transport: QuicVideoTransport,
    config: HostConfig,
    token: CancellationToken,
) -> Result<()> {
    crate::host_capture_macos::stream(transport, config, token).await
}

/// Non-Windows/non-macOS streaming path — uses the software fallback pipeline
/// (scrap capturer + openh264/ffmpeg encoder) via the factory functions.
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub(crate) async fn stream(
    transport: QuicVideoTransport,
    config: HostConfig,
    token: CancellationToken,
) -> Result<()> {
    use rayplay_video::{CaptureConfig, create_capturer, encoder::create_encoder};

    let cap_config = CaptureConfig {
        target_fps: config.encoder_config.fps,
        acquire_timeout_ms: 100,
    };
    let capturer =
        create_capturer(cap_config, config.pipeline_mode).map_err(anyhow::Error::from)?;
    let (cap_width, cap_height) = capturer.resolution();

    let enc_config = EncoderConfig::new(cap_width, cap_height, config.encoder_config.fps)
        .with_bitrate(config.encoder_config.bitrate);
    let encoder = create_encoder(enc_config, config.pipeline_mode).map_err(anyhow::Error::from)?;

    stream_with_pipeline(transport, capturer, encoder, token).await
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;
