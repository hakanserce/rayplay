//! Core streaming logic and shared traits for `RayPlay`.

pub mod frame;
pub mod packet;
pub mod pairing;
pub mod session;

pub use frame::RawFrame;
pub use packet::EncodedPacket;
pub use pairing::{PairingError, TrustDatabase, TrustedClient};
pub use session::{ControlMessage, PairingOutcome, SessionError, SessionState, StreamParams};

use std::future::Future;
use thiserror::Error;

/// Errors produced by the network transport layer.
#[derive(Debug, Error)]
pub enum NetworkError {
    /// A generic transport-level error with a descriptive message.
    #[error("transport error: {0}")]
    Transport(String),
    /// The connection was closed by the remote peer.
    #[error("connection closed")]
    ConnectionClosed,
    /// The local endpoint was shut down.
    #[error("endpoint closed")]
    EndpointClosed,
}

/// Platform-agnostic network transport abstraction.
///
/// Implementations live in `rayplay-network`. This trait keeps
/// `rayplay-video` and `rayplay-input` independent of `quinn`.
pub trait NetworkTransport: Send {
    /// Sends an encoded video packet to the remote peer.
    fn send_video(
        &mut self,
        packet: &EncodedPacket,
    ) -> impl Future<Output = Result<(), NetworkError>> + Send;

    /// Receives the next reassembled video packet from the remote peer.
    fn recv_video(&mut self) -> impl Future<Output = Result<EncodedPacket, NetworkError>> + Send;
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
