//! Host-side pairing/auth glue for the CLI (UC-016).
//!
//! Wires together the library-level pairing functions with the CLI's
//! accept loop.  Excluded from coverage because it performs trust-DB
//! persistence and PIN display I/O.

use anyhow::Result;
use rayplay_core::pairing::TrustDatabase;
use rayplay_core::session::{ClientIntent, ControlMessage, StreamParams};
use rayplay_network::{QuicVideoTransport, host_auth_challenge, host_handshake, host_pairing};
use tokio_util::sync::CancellationToken;

use crate::host::HostConfig;

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
                    stream_with_handshake(transport, config, control, token).await
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

            stream_with_handshake(transport, config, control, token).await
        }
    }
}

/// Performs codec negotiation handshake then streams with the negotiated codec.
async fn stream_with_handshake(
    transport: QuicVideoTransport,
    config: HostConfig,
    mut control: rayplay_network::ControlChannel,
    token: CancellationToken,
) -> Result<()> {
    // On Windows Auto mode, use the zero-copy DXGI + NVENC pipeline.
    // On Windows Software mode (or non-Windows), use the generic pipeline.
    #[cfg(target_os = "windows")]
    if config.pipeline_mode == rayplay_video::PipelineMode::Auto {
        return stream_windows_zero_copy(transport, config, &mut control, token).await;
    }

    let (capturer, encoder) = build_generic_pipeline(&config).await?;

    negotiate_and_stream(transport, &mut control, capturer, encoder, token).await
}

/// Windows zero-copy pipeline: DXGI capture + NVENC hardware encoder.
#[cfg(target_os = "windows")]
async fn stream_windows_zero_copy(
    transport: QuicVideoTransport,
    config: HostConfig,
    control: &mut rayplay_network::ControlChannel,
    token: CancellationToken,
) -> Result<()> {
    use rayplay_video::{
        CaptureConfig, SharedD3D11Device,
        capture::ZeroCopyCapturer,
        dxgi_capture::DxgiCapture,
        encoder::{EncoderConfig, VideoEncoder as _},
        nvenc::NvencEncoder,
    };
    use std::sync::Arc;

    let device = Arc::new(SharedD3D11Device::new().map_err(anyhow::Error::from)?);
    let cap_config = CaptureConfig {
        target_fps: config.encoder_config.fps,
        acquire_timeout_ms: 100,
    };
    let capturer = DxgiCapture::new(cap_config, device.clone()).map_err(anyhow::Error::from)?;
    let (cap_width, cap_height) = <DxgiCapture as ZeroCopyCapturer>::resolution(&capturer);

    let enc_config = EncoderConfig::new(cap_width, cap_height, config.encoder_config.fps)
        .with_bitrate(config.encoder_config.bitrate);
    let encoder =
        NvencEncoder::new(enc_config, device.device_ptr()).map_err(anyhow::Error::from)?;

    let actual_codec = encoder.config().codec;
    let stream_params = StreamParams {
        width: cap_width,
        height: cap_height,
        fps: config.encoder_config.fps,
        codec: actual_codec.to_string(),
    };
    let _agreed = host_handshake(control, |_proposed| stream_params.clone())
        .await
        .map_err(|e| anyhow::anyhow!("handshake failed: {e}"))?;
    tracing::info!(codec = %actual_codec, width = cap_width, height = cap_height, "Codec negotiation complete");

    crate::host::stream_with_zero_copy_pipeline(transport, capturer, encoder, token).await
}

/// Builds the capturer + encoder pair using the platform-appropriate path.
#[allow(clippy::unused_async)] // async needed on macOS for screen recording permission check
async fn build_generic_pipeline(
    config: &HostConfig,
) -> Result<(
    Box<dyn rayplay_video::capture::ScreenCapturer>,
    Box<dyn rayplay_video::encoder::VideoEncoder>,
)> {
    use rayplay_video::{
        CaptureConfig, create_capturer,
        encoder::{EncoderConfig, create_encoder},
    };

    // macOS requires a screen recording permission check before capture.
    #[cfg(target_os = "macos")]
    crate::host_capture_macos::wait_for_screen_recording_permission().await?;

    let cap_config = CaptureConfig {
        target_fps: config.encoder_config.fps,
        acquire_timeout_ms: 100,
    };
    let capturer =
        create_capturer(cap_config, config.pipeline_mode).map_err(anyhow::Error::from)?;
    let (cap_width, cap_height) = capturer.resolution();

    let enc_config = EncoderConfig::new(cap_width, cap_height, config.encoder_config.fps)
        .with_bitrate(config.encoder_config.bitrate.clone());
    let encoder = create_encoder(enc_config, config.pipeline_mode).map_err(anyhow::Error::from)?;

    Ok((capturer, encoder))
}

/// Runs the codec handshake and then streams using the generic (boxed) pipeline.
async fn negotiate_and_stream(
    transport: QuicVideoTransport,
    control: &mut rayplay_network::ControlChannel,
    capturer: Box<dyn rayplay_video::capture::ScreenCapturer>,
    encoder: Box<dyn rayplay_video::encoder::VideoEncoder>,
    token: CancellationToken,
) -> Result<()> {
    let actual_codec = encoder.config().codec;
    let stream_params = StreamParams {
        width: encoder.config().width,
        height: encoder.config().height,
        fps: encoder.config().fps,
        codec: actual_codec.to_string(),
    };

    let _agreed = host_handshake(control, |_proposed| stream_params.clone())
        .await
        .map_err(|e| anyhow::anyhow!("handshake failed: {e}"))?;

    tracing::info!(
        codec = %actual_codec,
        width = encoder.config().width,
        height = encoder.config().height,
        fps = encoder.config().fps,
        "Codec negotiation complete"
    );

    crate::host::stream_with_pipeline(transport, capturer, encoder, token).await
}

/// Best-effort save of the trust database to disk.
async fn save_trust_db_if_possible(trust_db: &std::sync::Arc<tokio::sync::Mutex<TrustDatabase>>) {
    let db = trust_db.lock().await;
    if let Err(e) = rayplay_network::trust_store::save_trust_db(&db) {
        tracing::warn!(error = %e, "Failed to persist trust database");
    }
}
