//! Pure synchronous decode-and-dispatch logic extracted from the receive loop.
//!
//! The async `tokio::select!` loop in `receive.rs` delegates to
//! [`decode_and_dispatch`] for the decode → channel-send step, keeping the
//! business logic testable without a QUIC transport or async runtime.

use crossbeam_channel::Sender;
use rayplay_video::{DecodedFrame, FrameNotifier, decoder::VideoDecoder, packet::EncodedPacket};

/// Outcome of decoding one packet and dispatching the resulting frame.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum DispatchResult {
    /// Frame decoded and sent to the renderer channel.
    Sent,
    /// Frame decoded but the renderer channel was full; frame dropped.
    Dropped,
    /// Frame decoded but the renderer channel is disconnected; caller should stop.
    ChannelClosed,
    /// Decoder buffered the packet but did not emit a frame yet.
    NoFrame,
    /// Decoder returned an error; packet skipped.
    DecodeError,
}

/// Decodes a single encoded packet and dispatches the resulting frame.
///
/// Returns a [`DispatchResult`] indicating what happened so the caller can
/// decide whether to continue or break out of the receive loop.
///
/// After a successful send, `notifier` wakes the `winit` event loop so it
/// picks up the frame immediately without busy-polling.
pub(crate) fn decode_and_dispatch(
    decoder: &mut dyn VideoDecoder,
    packet: &EncodedPacket,
    frame_tx: &Sender<DecodedFrame>,
    notifier: &FrameNotifier,
) -> DispatchResult {
    match decoder.decode(packet) {
        Ok(Some(frame)) => {
            tracing::debug!(
                timestamp_us = frame.timestamp_us,
                width = frame.width,
                height = frame.height,
                "frame_decoded"
            );
            match frame_tx.try_send(frame) {
                Ok(()) => {
                    notifier.notify();
                    DispatchResult::Sent
                }
                Err(crossbeam_channel::TrySendError::Full(_)) => {
                    tracing::trace!("Renderer is behind, frame dropped");
                    DispatchResult::Dropped
                }
                Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                    tracing::debug!("Frame channel closed, stopping receive loop");
                    DispatchResult::ChannelClosed
                }
            }
        }
        Ok(None) => DispatchResult::NoFrame,
        Err(e) => {
            tracing::warn!(error = %e, "Decode error, skipping packet");
            DispatchResult::DecodeError
        }
    }
}

#[cfg(test)]
mod tests;
