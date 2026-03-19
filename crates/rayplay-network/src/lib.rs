//! Network transport layer for `RayPlay` (UC-003).
//!
//! Implements low-latency QUIC video stream transport using RFC 9221
//! unreliable datagrams via `quinn` ≥0.11.
//!
//! # Pipeline
//!
//! ```text
//! Host:   EncodedPacket → FrameFragmenter → QUIC datagrams ──UDP──►
//! Client: ◄──UDP── QUIC datagrams → FrameReassembler → EncodedPacket
//! ```

pub mod fragmenter;
pub mod reassembler;
pub mod transport;
pub(crate) mod transport_tls;
pub mod wire;

pub use fragmenter::FrameFragmenter;
pub use reassembler::FrameReassembler;
pub use transport::{QuicListener, QuicVideoTransport};
pub use wire::{
    Channel, FLAG_KEYFRAME, HEADER_LEN, MAX_FRAGMENT_PAYLOAD, TransportError, VideoFragment,
};