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

    /// Filesystem storage error (trust database, client key persistence).
    #[error("storage error: {0}")]
    StorageError(String),

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

    /// QUIC stream write error on the control channel.
    #[error("stream write error: {0}")]
    StreamWrite(String),

    /// QUIC stream read error on the control channel.
    #[error("stream read error: {0}")]
    StreamRead(String),

    /// A control message exceeds the maximum allowed size.
    #[error("control message exceeds max size: {0} bytes")]
    MessageTooLarge(usize),

    /// A control message could not be deserialized from JSON.
    #[error("control message parse error: {0}")]
    MessageParse(String),
}

/// A single video fragment as exchanged over QUIC unreliable datagrams.
///
/// Each [`EncodedPacket`] is split into one or more `VideoFragment`s by
/// [`VideoFragmenter`] and reassembled back into an [`EncodedPacket`] by
/// [`VideoReassembler`].
///
/// [`EncodedPacket`]: rayplay_core::packet::EncodedPacket
/// [`VideoFragmenter`]: crate::fragmenter::VideoFragmenter
/// [`VideoReassembler`]: crate::reassembler::VideoReassembler
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
        // bytes 10–11 are reserved; ignore on decode

        if frag_total == 0 {
            return Err(TransportError::InvalidFragTotal);
        }
        if frag_index >= frag_total {
            return Err(TransportError::FragIndexOutOfRange {
                frag_index,
                frag_total,
            });
        }

        let payload = datagram[HEADER_LEN..].to_vec();

        Ok(Self {
            frame_id,
            frag_index,
            frag_total,
            channel,
            flags,
            payload,
        })
    }

    /// Returns `true` if this fragment belongs to a keyframe (IDR).
    #[must_use]
    pub fn is_keyframe(&self) -> bool {
        self.flags & FLAG_KEYFRAME != 0
    }
}

#[cfg(test)]
mod tests;
