//! Wire protocol types for the QUIC video transport layer (ADR-003).
//!
//! Defines the 12-byte fragment header, [`VideoFragment`] encode/decode,
//! the [`Channel`] enum, and [`TransportError`].
//!
//! # Header layout (big-endian)
//!
//! ```text
//! 0       4       6       8   9    10      12
//! |frame_id|frag_idx|frag_tot|chan|flags|reserved|
//! ```

use bytes::{BufMut, Bytes, BytesMut};
use thiserror::Error;

/// Length of the fixed wire header in bytes.
pub const HEADER_LEN: usize = 12;

/// Maximum payload bytes per fragment (1200 byte QUIC datagram − 12 byte header).
pub const MAX_FRAGMENT_PAYLOAD: usize = 1200 - HEADER_LEN;

/// Bit 0 of the `flags` byte: this fragment belongs to an IDR (keyframe).
pub const FLAG_KEYFRAME: u8 = 0b0000_0001;

/// Logical channel carried by a fragment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Channel {
    /// Primary video stream.
    Video = 0,
}

impl TryFrom<u8> for Channel {
    type Error = TransportError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Video),
            other => Err(TransportError::UnknownChannel(other)),
        }
    }
}

/// Errors produced by wire encoding / decoding and the QUIC transport.
#[derive(Debug, Error)]
pub enum TransportError {
    /// Datagram is too short to contain a full header.
    #[error("datagram too short: {0} bytes (need {HEADER_LEN})")]
    DatagramTooShort(usize),

    /// `frag_total` field is zero, which is invalid.
    #[error("frag_total must be > 0")]
    InvalidFragTotal,

    /// `frag_index` is ≥ `frag_total`.
    #[error("frag_index {frag_index} out of range for frag_total {frag_total}")]
    FragIndexOutOfRange {
        /// The received fragment index.
        frag_index: u16,
        /// The declared total fragment count.
        frag_total: u16,
    },

    /// Unknown channel discriminant.
    #[error("unknown channel: {0}")]
    UnknownChannel(u8),

    /// TLS configuration error.
    #[error("TLS error: {0}")]
    TlsError(String),

    /// The QUIC endpoint was closed before a connection could be accepted.
    #[error("endpoint closed")]
    EndpointClosed,

    /// QUIC connection-level error.
    #[error("connection error: {0}")]
    Connection(#[from] quinn::ConnectionError),

    /// QUIC connect error (failed to initiate a connection).
    #[error("connect error: {0}")]
    Connect(#[from] quinn::ConnectError),

    /// Failed to send a datagram over the QUIC connection.
    #[error("send datagram error: {0}")]
    SendDatagram(#[from] quinn::SendDatagramError),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// A single video fragment as exchanged over QUIC unreliable datagrams.
///
/// Each [`EncodedPacket`] is split into one or more `VideoFragment`s by
/// [`FrameFragmenter`] and reassembled back into an [`EncodedPacket`] by
/// [`FrameReassembler`].
///
/// [`EncodedPacket`]: rayplay_core::packet::EncodedPacket
/// [`FrameFragmenter`]: crate::fragmenter::FrameFragmenter
/// [`FrameReassembler`]: crate::reassembler::FrameReassembler
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoFragment {
    /// Monotonically-increasing frame identifier (wraps at `u32::MAX`).
    pub frame_id: u32,
    /// Zero-based index of this fragment within the frame.
    pub frag_index: u16,
    /// Total number of fragments that make up this frame (≥1).
    pub frag_total: u16,
    /// Logical channel (always [`Channel::Video`] for now).
    pub channel: Channel,
    /// Flags byte; bit 0 is [`FLAG_KEYFRAME`].
    pub flags: u8,
    /// Raw payload bytes (at most [`MAX_FRAGMENT_PAYLOAD`] bytes).
    pub payload: Vec<u8>,
}

impl VideoFragment {
    /// Encodes the fragment into a [`Bytes`] buffer ready to send as a QUIC datagram.
    ///
    /// Layout: 12-byte header followed by the raw payload.
    #[must_use]
    pub fn encode(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(HEADER_LEN + self.payload.len());
        buf.put_u32(self.frame_id);
        buf.put_u16(self.frag_index);
        buf.put_u16(self.frag_total);
        buf.put_u8(self.channel as u8);
        buf.put_u8(self.flags);
        buf.put_u16(0u16); // reserved
        buf.put_slice(&self.payload);
        buf.freeze()
    }

    /// Decodes a QUIC datagram into a [`VideoFragment`].
    ///
    /// # Errors
    ///
    /// - [`TransportError::DatagramTooShort`] if `datagram` is shorter than [`HEADER_LEN`].
    /// - [`TransportError::InvalidFragTotal`] if `frag_total == 0`.
    /// - [`TransportError::FragIndexOutOfRange`] if `frag_index >= frag_total`.
    /// - [`TransportError::UnknownChannel`] if the channel byte is unrecognized.
    pub fn decode(datagram: &[u8]) -> Result<Self, TransportError> {
        if datagram.len() < HEADER_LEN {
            return Err(TransportError::DatagramTooShort(datagram.len()));
        }

        let frame_id = u32::from_be_bytes([datagram[0], datagram[1], datagram[2], datagram[3]]);
        let frag_index = u16::from_be_bytes([datagram[4], datagram[5]]);
        let frag_total = u16::from_be_bytes([datagram[6], datagram[7]]);
        let channel = Channel::try_from(datagram[8])?;
        let flags = datagram[9];
        // Skip datagram[10..12] (reserved bytes)
        let payload = datagram[HEADER_LEN..].to_vec();

        if frag_total == 0 {
            return Err(TransportError::InvalidFragTotal);
        }
        if frag_index >= frag_total {
            return Err(TransportError::FragIndexOutOfRange {
                frag_index,
                frag_total,
            });
        }

        Ok(Self {
            frame_id,
            frag_index,
            frag_total,
            channel,
            flags,
            payload,
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── VideoFragment::encode/decode ──────────────────────────────────────────

    fn create_test_fragment(
        frame_id: u32,
        frag_index: u16,
        frag_total: u16,
        flags: u8,
        payload: Vec<u8>,
    ) -> VideoFragment {
        VideoFragment {
            frame_id,
            frag_index,
            frag_total,
            channel: Channel::Video,
            flags,
            payload,
        }
    }

    // ── Channel ───────────────────────────────────────────────────────────────

    #[test]
    fn test_channel_try_from_zero_is_video() {
        assert_eq!(Channel::try_from(0u8).unwrap(), Channel::Video);
    }

    #[test]
    fn test_channel_try_from_unknown_returns_error() {
        let result = Channel::try_from(255u8);
        assert!(matches!(result, Err(TransportError::UnknownChannel(255))));
    }

    // ── VideoFragment ─────────────────────────────────────────────────────────

    #[test]
    fn test_video_fragment_encode_decode_roundtrip() {
        let frag = create_test_fragment(12345, 2, 5, FLAG_KEYFRAME, vec![0xAB, 0xCD, 0xEF]);
        let encoded = frag.encode();
        let decoded = VideoFragment::decode(&encoded).unwrap();
        assert_eq!(decoded, frag);
    }

    #[test]
    fn test_video_fragment_encode_header_layout() {
        let frag = create_test_fragment(0x12345678, 0x9ABC, 0xDEF0, 0x42, vec![]);
        let encoded = frag.encode();

        assert_eq!(encoded.len(), HEADER_LEN); // No payload
        assert_eq!(&encoded[0..4], &[0x12, 0x34, 0x56, 0x78]); // frame_id (BE)
        assert_eq!(&encoded[4..6], &[0x9A, 0xBC]); // frag_index (BE)
        assert_eq!(&encoded[6..8], &[0xDE, 0xF0]); // frag_total (BE)
        assert_eq!(encoded[8], 0); // channel = Video
        assert_eq!(encoded[9], 0x42); // flags
        assert_eq!(&encoded[10..12], &[0, 0]); // reserved
    }

    #[test]
    fn test_video_fragment_encode_includes_payload() {
        let payload = vec![1u8, 2, 3, 4];
        let frag = create_test_fragment(100, 0, 1, 0, payload.clone());
        let encoded = frag.encode();

        assert_eq!(encoded.len(), HEADER_LEN + 4);
        assert_eq!(&encoded[HEADER_LEN..], &payload);
    }

    #[test]
    fn test_video_fragment_decode_too_short_returns_error() {
        let short = vec![0u8; HEADER_LEN - 1];
        let result = VideoFragment::decode(&short);
        assert!(matches!(result, Err(TransportError::DatagramTooShort(11))));
    }

    #[test]
    fn test_video_fragment_decode_zero_frag_total_returns_error() {
        let mut buf = vec![0u8; HEADER_LEN];
        buf[6] = 0; // frag_total = 0
        buf[7] = 0;
        let result = VideoFragment::decode(&buf);
        assert!(matches!(result, Err(TransportError::InvalidFragTotal)));
    }

    #[test]
    fn test_video_fragment_decode_frag_index_out_of_range_returns_error() {
        let mut buf = vec![0u8; HEADER_LEN];
        buf[4] = 0; // frag_index = 5
        buf[5] = 5;
        buf[6] = 0; // frag_total = 3
        buf[7] = 3;
        let result = VideoFragment::decode(&buf);
        assert!(matches!(
            result,
            Err(TransportError::FragIndexOutOfRange {
                frag_index: 5,
                frag_total: 3
            })
        ));
    }

    #[test]
    fn test_video_fragment_decode_unknown_channel_returns_error() {
        let mut buf = vec![0u8; HEADER_LEN];
        buf[6] = 0; // frag_total = 1 (valid)
        buf[7] = 1;
        buf[8] = 99; // unknown channel
        let result = VideoFragment::decode(&buf);
        assert!(matches!(result, Err(TransportError::UnknownChannel(99))));
    }

    #[test]
    fn test_video_fragment_decode_with_payload() {
        let payload = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let mut buf = vec![0u8; HEADER_LEN];
        buf[6] = 0; // frag_total = 1
        buf[7] = 1;
        buf.extend(&payload);

        let decoded = VideoFragment::decode(&buf).unwrap();
        assert_eq!(decoded.payload, payload);
    }

    #[test]
    fn test_video_fragment_decode_preserves_flags() {
        let mut buf = vec![0u8; HEADER_LEN];
        buf[6] = 0; // frag_total = 1
        buf[7] = 1;
        buf[9] = FLAG_KEYFRAME; // flags

        let decoded = VideoFragment::decode(&buf).unwrap();
        assert_eq!(decoded.flags, FLAG_KEYFRAME);
    }

    // ── TransportError ────────────────────────────────────────────────────────

    #[test]
    fn test_transport_error_datagram_too_short_display() {
        let err = TransportError::DatagramTooShort(8);
        assert_eq!(err.to_string(), "datagram too short: 8 bytes (need 12)");
    }

    #[test]
    fn test_transport_error_invalid_frag_total_display() {
        let err = TransportError::InvalidFragTotal;
        assert_eq!(err.to_string(), "frag_total must be > 0");
    }

    #[test]
    fn test_transport_error_frag_index_out_of_range_display() {
        let err = TransportError::FragIndexOutOfRange {
            frag_index: 10,
            frag_total: 5,
        };
        assert_eq!(
            err.to_string(),
            "frag_index 10 out of range for frag_total 5"
        );
    }

    #[test]
    fn test_transport_error_unknown_channel_display() {
        let err = TransportError::UnknownChannel(42);
        assert_eq!(err.to_string(), "unknown channel: 42");
    }

    #[test]
    fn test_transport_error_tls_error_display() {
        let err = TransportError::TlsError("bad cert".to_string());
        assert_eq!(err.to_string(), "TLS error: bad cert");
    }
}
