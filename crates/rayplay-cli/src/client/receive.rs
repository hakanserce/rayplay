//! Receive-decode loop for the `rayview` client (UC-007).

use anyhow::Result;
use crossbeam_channel::Sender;
use rayplay_network::QuicVideoTransport;
use rayplay_video::{DecodedFrame, decoder::VideoDecoder};

/// Receives encoded packets from `transport`, decodes them, and forwards decoded
/// frames to `frame_tx` until shutdown or the rendering channel disconnects.
///
/// When the renderer is behind, frames are dropped with a trace-level log
/// (non-blocking `try_send`) to preserve the low-latency pipeline.
/// Decode errors are logged as warnings and the packet is skipped; only network
/// errors are fatal and returned as `Err`.
///
/// # Errors
///
/// Returns `Err` if `transport.recv_video` fails with a network error.
pub(crate) async fn run_receive_loop(
    mut transport: QuicVideoTransport,
    mut decoder: Box<dyn VideoDecoder>,
    frame_tx: Sender<DecodedFrame>,
    mut shutdown: tokio::sync::oneshot::Receiver<()>,
) -> Result<()> {
    loop {
        let packet = tokio::select! {
            _ = &mut shutdown => break,
            result = transport.recv_video() => result.map_err(anyhow::Error::from)?,
        };

        match decoder.decode(&packet) {
            Ok(Some(frame)) => match frame_tx.try_send(frame) {
                Ok(()) => {}
                Err(crossbeam_channel::TrySendError::Full(_)) => {
                    tracing::trace!("Renderer is behind, frame dropped");
                }
                Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                    tracing::debug!("Frame channel closed, stopping receive loop");
                    break;
                }
            },
            Ok(None) => {}
            Err(e) => tracing::warn!(error = %e, "Decode error, skipping packet"),
        }
    }
    Ok(())
}
