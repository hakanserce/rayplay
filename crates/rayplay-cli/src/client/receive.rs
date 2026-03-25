//! Receive-decode loop for the `rayview` client (UC-007, UC-008).

use anyhow::Result;
use crossbeam_channel::Sender;
use rayplay_network::QuicVideoTransport;
use rayplay_video::{DecodedFrame, decoder::VideoDecoder};
use tokio_util::sync::CancellationToken;

use super::decode_dispatch::{DispatchResult, decode_and_dispatch};

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
    token: CancellationToken,
) -> Result<()> {
    loop {
        let packet = tokio::select! {
            () = token.cancelled() => break,
            result = transport.recv_video() => result.map_err(|e| anyhow::anyhow!(
                "video stream closed by host (server may have failed to start encoding pipeline): {e}"
            ))?,
        };

        tracing::debug!(size = packet.data.len(), "packet_received");

        if decode_and_dispatch(&mut *decoder, &packet, &frame_tx) == DispatchResult::ChannelClosed {
            break;
        }
    }
    Ok(())
}
