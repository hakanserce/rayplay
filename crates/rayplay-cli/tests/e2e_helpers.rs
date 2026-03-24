//! Shared test helpers for end-to-end integration tests.
#![allow(
    clippy::must_use_candidate,
    clippy::missing_panics_doc,
    clippy::missing_errors_doc,
    clippy::similar_names
)]

use std::time::{Duration, Instant};

use anyhow::Result;
use crossbeam_channel::Receiver;
use rayplay_network::QuicVideoTransport;
#[allow(unused_imports)]
use rayplay_video::{
    DecodedFrame, EncodedPacket, RawFrame, Renderer, WgpuRenderer,
    capture::{CaptureError, CapturedFrame, ScreenCapturer},
    decoder::VideoDecoder,
    encoder::{EncoderInput, VideoEncoder},
};
use tokio_util::sync::CancellationToken;

/// Synthetic screen capturer that generates solid-color BGRA frames.
///
/// Produces `frame_count` frames, then returns `CaptureError::Timeout` forever.
pub struct SyntheticCapturer {
    width: u32,
    height: u32,
    frames_remaining: u32,
}

impl SyntheticCapturer {
    pub fn new(width: u32, height: u32, frame_count: u32) -> Self {
        Self {
            width,
            height,
            frames_remaining: frame_count,
        }
    }
}

impl ScreenCapturer for SyntheticCapturer {
    fn capture_frame(&mut self) -> Result<CapturedFrame, CaptureError> {
        if self.frames_remaining == 0 {
            return Err(CaptureError::Timeout(Duration::from_millis(100)));
        }
        self.frames_remaining -= 1;

        let stride = self.width * 4;
        let data = vec![128u8; (stride * self.height) as usize];

        Ok(CapturedFrame {
            width: self.width,
            height: self.height,
            stride,
            data,
            timestamp: Instant::now(),
        })
    }

    fn resolution(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

/// Creates a QUIC loopback transport pair for testing.
///
/// Returns `(server_transport, client_transport)`.
#[allow(dead_code)]
pub async fn setup_transport() -> Result<(QuicVideoTransport, QuicVideoTransport)> {
    let bind_addr = "127.0.0.1:0".parse()?;
    let (listener, cert_der) = QuicVideoTransport::listen(bind_addr)?;
    let server_addr = listener.local_addr()?;

    let server_task = tokio::spawn(async move { listener.accept().await });
    let client = QuicVideoTransport::connect(server_addr, cert_der).await?;
    let server = server_task.await??;

    Ok((server, client))
}

/// Drives capture → encode → send loop similar to the production host.
///
/// Spawns a blocking thread for capture+encode, forwards packets to an async
/// send loop via tokio mpsc channel.
#[allow(clippy::cast_possible_truncation)]
pub async fn run_host_encode_send(
    mut transport: QuicVideoTransport,
    capturer: Box<dyn ScreenCapturer>,
    encoder: Box<dyn VideoEncoder>,
    token: CancellationToken,
) -> Result<()> {
    let (packet_tx, mut packet_rx) = tokio::sync::mpsc::channel::<Result<EncodedPacket>>(4);
    let encode_token = token.clone();

    let encode_handle = tokio::task::spawn_blocking(move || {
        let mut capturer = capturer;
        let mut encoder = encoder;
        let session_start = Instant::now();

        loop {
            if encode_token.is_cancelled() {
                break;
            }

            let captured = match capturer.capture_frame() {
                Ok(frame) => frame,
                Err(CaptureError::Timeout(_)) => {
                    if packet_tx.is_closed() {
                        break;
                    }
                    continue;
                }
                Err(e) => {
                    let _ = packet_tx.blocking_send(Err(e.into()));
                    return;
                }
            };

            let timestamp_us = session_start.elapsed().as_micros() as u64;
            let raw = RawFrame::new(
                captured.data,
                captured.width,
                captured.height,
                captured.stride,
                timestamp_us,
            );

            match encoder.encode(EncoderInput::Cpu(&raw)) {
                Ok(Some(packet)) => {
                    if packet_tx.blocking_send(Ok(packet)).is_err() {
                        break;
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    let _ = packet_tx.blocking_send(Err(e.into()));
                    return;
                }
            }
        }
    });

    // Async send loop
    loop {
        tokio::select! {
            () = token.cancelled() => break,
            packet = packet_rx.recv() => {
                match packet {
                    Some(Ok(p)) => {
                        transport.send_video(&p).await?;
                    }
                    Some(Err(e)) => return Err(e),
                    None => break,
                }
            }
        }
    }

    encode_handle
        .await
        .map_err(|e| anyhow::anyhow!("encode thread panicked: {e}"))?;
    Ok(())
}

/// Drives recv → decode → channel-send loop similar to the production client.
#[allow(dead_code)]
pub async fn run_client_recv_decode(
    mut transport: QuicVideoTransport,
    mut decoder: Box<dyn VideoDecoder>,
    frame_tx: crossbeam_channel::Sender<DecodedFrame>,
    token: CancellationToken,
) -> Result<()> {
    loop {
        tokio::select! {
            result = transport.recv_video() => {
                let packet = result?;
                match decoder.decode(&packet) {
                    Ok(Some(frame)) => {
                        if frame_tx.try_send(frame).is_err() {
                            break;
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        tracing::warn!(error = %e, "decode error in test, skipping");
                    }
                }
            }
            () = token.cancelled() => break,
        }
    }
    Ok(())
}

/// Creates a headless wgpu device + queue for offscreen rendering in tests.
#[allow(dead_code)]
pub fn create_headless_device() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });
    pollster::block_on(async {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .expect("no GPU adapter — run with a Metal-capable device");
        adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
            .expect("device creation failed")
    })
}

/// Drives recv → decode → render → channel-send loop.
///
/// Like [`run_client_recv_decode`] but also renders each decoded frame through
/// an offscreen [`WgpuRenderer`], exercising the full `VtDecoder` → `IOSurface` →
/// Metal → wgpu render path that the production client uses.
#[allow(dead_code)]
pub async fn run_client_recv_decode_render(
    mut transport: QuicVideoTransport,
    mut decoder: Box<dyn VideoDecoder>,
    frame_tx: crossbeam_channel::Sender<DecodedFrame>,
    token: CancellationToken,
    width: u32,
    height: u32,
) -> Result<()> {
    let (device, queue) = create_headless_device();
    let mut renderer = WgpuRenderer::new_offscreen(device, queue, width, height);

    loop {
        tokio::select! {
            result = transport.recv_video() => {
                let packet = result?;
                match decoder.decode(&packet) {
                    Ok(Some(frame)) => {
                        renderer.present_frame(&frame)?;
                        if frame_tx.try_send(frame).is_err() {
                            break;
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        tracing::warn!(error = %e, "decode error in test, skipping");
                    }
                }
            }
            () = token.cancelled() => break,
        }
    }
    Ok(())
}

/// Collects decoded frames from the channel with a timeout.
pub fn collect_frames(
    rx: &Receiver<DecodedFrame>,
    expected: usize,
    timeout: Duration,
) -> Vec<DecodedFrame> {
    let mut frames = Vec::new();
    let deadline = Instant::now() + timeout;

    while frames.len() < expected && Instant::now() < deadline {
        if let Ok(frame) = rx.try_recv() {
            frames.push(frame);
        } else {
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    frames
}
