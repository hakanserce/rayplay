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

/// Decoder that returns [`VideoError::CorruptPacket`] for packets whose first byte is `0xDE`
/// and emits a 1×1 frame for all other packets.
///
/// Used to verify that the receive loop skips decode errors and continues running.
pub(crate) struct SkipBadDecoder;

impl VideoDecoder for SkipBadDecoder {
    fn decode(&mut self, packet: &EncodedPacket) -> Result<Option<DecodedFrame>, VideoError> {
        if packet.data.first() == Some(&0xDE) {
            return Err(VideoError::CorruptPacket {
                reason: "test corrupt".to_string(),
            });
        }
        Ok(Some(DecodedFrame::new_cpu(
            vec![0u8; 4],
            1,
            1,
            4,
            PixelFormat::Bgra8,
            0,
        )))
    }

    fn flush(&mut self) -> Result<Vec<DecodedFrame>, VideoError> {
        Ok(vec![])
    }

    fn codec(&self) -> Codec {
        Codec::Hevc
    }
}

// ── Stub coverage ─────────────────────────────────────────────────────────────

#[test]
fn test_null_decoder_flush_and_codec() {
    let mut d = NullDecoder {
        emit: false,
        fail: false,
    };
    assert!(d.flush().unwrap().is_empty());
    assert_eq!(d.codec(), Codec::Hevc);
}

#[test]
fn test_null_decoder_fail_returns_corrupt_packet() {
    use rayplay_video::decoder::VideoDecoder;

    let mut d = NullDecoder {
        emit: false,
        fail: true,
    };
    let pkt = EncodedPacket::new(vec![1], false, 0, 0);
    assert!(d.decode(&pkt).is_err());
}

#[test]
fn test_skip_bad_decoder_flush_and_codec() {
    let mut d = SkipBadDecoder;
    assert!(d.flush().unwrap().is_empty());
    assert_eq!(d.codec(), Codec::Hevc);
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Binds a loopback QUIC listener and returns it together with the server
/// certificate bytes and the bound address.
pub(crate) fn loopback_listener() -> (QuicListener, Vec<u8>, SocketAddr) {
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (listener, cert) = QuicVideoTransport::listen(bind).unwrap();
    let addr = listener.local_addr().unwrap();
    (listener, cert, addr)
}
