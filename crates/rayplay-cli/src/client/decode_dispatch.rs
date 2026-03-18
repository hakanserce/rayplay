//! Pure synchronous decode-and-dispatch logic extracted from the receive loop.
//!
//! The async `tokio::select!` loop in `receive.rs` delegates to
//! [`decode_and_dispatch`] for the decode → channel-send step, keeping the
//! business logic testable without a QUIC transport or async runtime.

use crossbeam_channel::Sender;
use rayplay_video::{DecodedFrame, decoder::VideoDecoder, packet::EncodedPacket};

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
pub(crate) fn decode_and_dispatch(
    decoder: &mut dyn VideoDecoder,
    packet: &EncodedPacket,
    frame_tx: &Sender<DecodedFrame>,
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
                Ok(()) => DispatchResult::Sent,
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
mod tests {
    use super::*;
    use crate::client::test_helper::NullDecoder;

    fn make_packet() -> EncodedPacket {
        EncodedPacket::new(vec![0u8; 4], false, 0, 0)
    }

    #[test]
    fn test_decode_and_dispatch_sent() {
        let mut decoder = NullDecoder {
            emit: true,
            fail: false,
        };
        let (tx, rx) = crossbeam_channel::bounded(1);
        let result = decode_and_dispatch(&mut decoder, &make_packet(), &tx);
        assert_eq!(result, DispatchResult::Sent);
        assert_eq!(rx.len(), 1);
    }

    #[test]
    fn test_decode_and_dispatch_channel_full() {
        let mut decoder = NullDecoder {
            emit: true,
            fail: false,
        };
        let (tx, _rx) = crossbeam_channel::bounded(0); // zero-capacity → always full
        let result = decode_and_dispatch(&mut decoder, &make_packet(), &tx);
        assert_eq!(result, DispatchResult::Dropped);
    }

    #[test]
    fn test_decode_and_dispatch_channel_disconnected() {
        let mut decoder = NullDecoder {
            emit: true,
            fail: false,
        };
        let (tx, rx) = crossbeam_channel::bounded(1);
        drop(rx); // disconnect
        let result = decode_and_dispatch(&mut decoder, &make_packet(), &tx);
        assert_eq!(result, DispatchResult::ChannelClosed);
    }

    #[test]
    fn test_decode_and_dispatch_no_frame() {
        let mut decoder = NullDecoder {
            emit: false,
            fail: false,
        };
        let (tx, _rx) = crossbeam_channel::bounded(1);
        let result = decode_and_dispatch(&mut decoder, &make_packet(), &tx);
        assert_eq!(result, DispatchResult::NoFrame);
    }

    #[test]
    fn test_decode_and_dispatch_decode_error() {
        let mut decoder = NullDecoder {
            emit: false,
            fail: true,
        };
        let (tx, _rx) = crossbeam_channel::bounded(1);
        let result = decode_and_dispatch(&mut decoder, &make_packet(), &tx);
        assert_eq!(result, DispatchResult::DecodeError);
    }
}
