//! Network transport layer for `RayPlay` (UC-003).
//!
//! Implements low-latency QUIC video stream transport using RFC 9221
//! unreliable datagrams via `quinn` ≥0.11.
//!
//! # Pipeline
//!
//! ```text
//! Host:   EncodedPacket → VideoFragmenter → QUIC datagrams ──UDP──►
//! Client: ◄──UDP── QUIC datagrams → VideoReassembler → EncodedPacket
//! ```

pub mod control;
pub mod fragmenter;
pub mod handshake;
pub mod keepalive;
pub mod pairing;
pub mod reassembler;
pub mod transport;
pub(crate) mod transport_tls;
pub mod trust_store;
pub mod wire;

pub use control::{ControlChannel, ControlReceiver, ControlSender};
pub use fragmenter::VideoFragmenter;
pub use handshake::{client_handshake, host_handshake};
pub use keepalive::{
    DEFAULT_KEEPALIVE_INTERVAL, DEFAULT_KEEPALIVE_TIMEOUT, run_keepalive_responder,
    run_keepalive_sender,
};
pub use pairing::{client_auth_response, client_pairing, host_auth_challenge, host_pairing};
pub use reassembler::VideoReassembler;
pub use transport::{QuicListener, QuicVideoTransport};
pub use wire::{
    Channel, FLAG_KEYFRAME, HEADER_LEN, MAX_FRAGMENT_PAYLOAD, TransportError, VideoFragment,
};
