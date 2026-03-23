//! Session control types for the `RayPlay` streaming protocol (UC-015, ADR-010).
//!
//! Defines the control messages exchanged over a reliable QUIC bidirectional
//! stream, stream parameter negotiation types, observable session state, and
//! session-level errors.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Stream parameters negotiated at connect time (UC-015 AC4).
///
/// Both sides must agree on these before media flows. The codec field is a
/// string (e.g. `"hevc"`, `"h264"`) to avoid coupling `rayplay-core` to
/// `rayplay-video`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamParams {
    /// Video width in pixels.
    pub width: u32,
    /// Video height in pixels.
    pub height: u32,
    /// Target frames per second.
    pub fps: u32,
    /// Codec identifier (e.g. `"hevc"`, `"h264"`).
    pub codec: String,
}

/// Outcome of a SPAKE2 pairing attempt (UC-016).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PairingOutcome {
    /// The client is now trusted and may connect without a PIN.
    Accepted,
    /// The pairing was rejected (wrong PIN, protocol error, etc.).
    Rejected(String),
}

/// Client intent declaration for disambiguation of auth vs. pairing flows (UC-016).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClientIntent {
    /// Client wants to perform PIN-based pairing.
    Pair,
    /// Client wants to authenticate as a trusted client.
    Auth,
}

/// Control messages exchanged over the reliable bidirectional QUIC stream.
///
/// Wire format: 4-byte little-endian length prefix followed by JSON payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlMessage {
    /// Client → Host: request these stream parameters.
    HandshakeRequest(StreamParams),
    /// Host → Client: agreed stream parameters.
    HandshakeResponse(StreamParams),
    /// Bidirectional keepalive ping.
    Keepalive,
    /// Bidirectional keepalive acknowledgement.
    KeepaliveAck,
    /// Bidirectional graceful disconnect signal (UC-015 AC5).
    Disconnect,
    /// Client → Host: declares intent (pairing or auth) to avoid deadlock.
    ClientHello(ClientIntent),
    /// Client → Host: SPAKE2 message for PIN-based pairing (UC-016).
    PairingRequest(Vec<u8>),
    /// Host → Client: SPAKE2 response message (UC-016).
    PairingResponse(Vec<u8>),
    /// Client → Host: HMAC confirmation with embedded public key (UC-016).
    PairingConfirm(Vec<u8>),
    /// Host → Client: pairing result (UC-016).
    PairingResult(PairingOutcome),
    /// Host → Client: random nonce for trusted-client auth (UC-016).
    AuthChallenge(Vec<u8>),
    /// Client → Host: public key + signed nonce (UC-016).
    AuthResponse(Vec<u8>),
    /// Host → Client: authentication succeeded or failed (UC-016).
    AuthResult(bool),
}

/// Observable session state for UI feedback (UC-015 AC3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// The session is active and media is flowing.
    Connected,
    /// The connection was lost and the client is attempting to reconnect.
    Reconnecting,
    /// The session has ended (timeout expired or explicit disconnect).
    Disconnected,
}

/// Errors from session-level operations.
#[derive(Debug, Error)]
pub enum SessionError {
    /// The handshake failed (unexpected message or protocol violation).
    #[error("handshake failed: {0}")]
    HandshakeFailed(String),
    /// The remote peer did not respond to keepalive in time.
    #[error("keepalive timeout")]
    KeepaliveTimeout,
    /// The remote peer closed the session.
    #[error("session closed by remote")]
    RemoteClosed,
    /// A control message could not be serialized or deserialized.
    #[error("serialization error: {0}")]
    Serialization(String),
    /// A transport-level error occurred on the control channel.
    #[error("transport error: {0}")]
    Transport(String),
    /// A pairing or authentication error (UC-016).
    #[error("pairing failed: {0}")]
    PairingFailed(String),
}

#[cfg(test)]
mod tests;
