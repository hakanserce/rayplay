//! Shared test doubles and helpers for the `rayview` client tests (UC-007).

use std::net::SocketAddr;

use rayplay_network::{QuicListener, QuicVideoTransport};
use rayplay_video::{
    DecodedFrame, PixelFormat,
    decoder::VideoDecoder,
    encoder::{Codec, VideoError},
    packet::EncodedPacket,
};

// ── Test double ───────────────────────────────────────────────────────────────

/// Minimal `VideoDecoder` test double.
pub(crate) struct NullDecoder {
    /// When `true`, `decode` returns a 1×1 frame.
    pub emit: bool,
    /// When `true`, `decode` returns `CorruptPacket`.
    pub fail: bool,
}

impl VideoDecoder for NullDecoder {
    fn decode(&mut self, packet: &EncodedPacket) -> Result<Option<DecodedFrame>, VideoError> {
        if self.fail {
            return Err(VideoError::CorruptPacket {
                reason: "test corrupt".to_string(),
            });
        }
        if self.emit {
            Ok(Some(DecodedFrame::new_cpu(
                vec![0u8; 4],
                1,
                1,
                4,
                PixelFormat::Bgra8,
                packet.timestamp_us,
            )))
        } else {
            Ok(None)
        }
    }

    fn flush(&mut self) -> Result<Vec<DecodedFrame>, VideoError> {
        Ok(vec![])
    }

    fn codec(&self) -> Codec {
        Codec::Hevc
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Binds a loopback QUIC listener and returns it together with the server
/// certificate bytes and the bound address.
pub(crate) fn loopback_listener() -> (QuicListener, Vec<u8>, SocketAddr) {
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (listener, cert) = QuicVideoTransport::listen(bind).unwrap();
    let addr = listener.local_addr().unwrap();
    (listener, cert.as_ref().to_vec(), addr)
}
