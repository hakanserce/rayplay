//! Session control types for the RayPlay streaming protocol (UC-015, ADR-010).
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_params() -> StreamParams {
        StreamParams {
            width: 1920,
            height: 1080,
            fps: 60,
            codec: "hevc".to_string(),
        }
    }

    // ── StreamParams ──────────────────────────────────────────────────────────

    #[test]
    fn test_stream_params_clone_equals_original() {
        let p = sample_params();
        assert_eq!(p.clone(), p);
    }

    #[test]
    fn test_stream_params_debug_contains_fields() {
        let p = sample_params();
        let dbg = format!("{p:?}");
        assert!(dbg.contains("1920"));
        assert!(dbg.contains("1080"));
        assert!(dbg.contains("60"));
        assert!(dbg.contains("hevc"));
    }

    #[test]
    fn test_stream_params_inequality_on_different_width() {
        let a = sample_params();
        let b = StreamParams { width: 1280, ..a.clone() };
        assert_ne!(a, b);
    }

    #[test]
    fn test_stream_params_inequality_on_different_codec() {
        let a = sample_params();
        let b = StreamParams {
            codec: "h264".to_string(),
            ..a.clone()
        };
        assert_ne!(a, b);
    }

    #[test]
    fn test_stream_params_serde_roundtrip() {
        let p = sample_params();
        let json = serde_json::to_string(&p).unwrap();
        let restored: StreamParams = serde_json::from_str(&json).unwrap();
        assert_eq!(p, restored);
    }

    #[test]
    fn test_stream_params_empty_codec_allowed() {
        let p = StreamParams {
            width: 0,
            height: 0,
            fps: 0,
            codec: String::new(),
        };
        let json = serde_json::to_string(&p).unwrap();
        let restored: StreamParams = serde_json::from_str(&json).unwrap();
        assert_eq!(p, restored);
    }

    // ── ControlMessage serde round-trips ──────────────────────────────────────

    #[test]
    fn test_serde_handshake_request() {
        let msg = ControlMessage::HandshakeRequest(sample_params());
        let json = serde_json::to_string(&msg).unwrap();
        let restored: ControlMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, restored);
    }

    #[test]
    fn test_serde_handshake_response() {
        let msg = ControlMessage::HandshakeResponse(sample_params());
        let json = serde_json::to_string(&msg).unwrap();
        let restored: ControlMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, restored);
    }

    #[test]
    fn test_serde_keepalive() {
        let msg = ControlMessage::Keepalive;
        let json = serde_json::to_string(&msg).unwrap();
        let restored: ControlMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, restored);
    }

    #[test]
    fn test_serde_keepalive_ack() {
        let msg = ControlMessage::KeepaliveAck;
        let json = serde_json::to_string(&msg).unwrap();
        let restored: ControlMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, restored);
    }

    #[test]
    fn test_serde_disconnect() {
        let msg = ControlMessage::Disconnect;
        let json = serde_json::to_string(&msg).unwrap();
        let restored: ControlMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, restored);
    }

    #[test]
    fn test_control_message_clone_equals_original() {
        let msg = ControlMessage::HandshakeRequest(sample_params());
        assert_eq!(msg.clone(), msg);
    }

    #[test]
    fn test_control_message_debug_contains_variant() {
        let msg = ControlMessage::Disconnect;
        assert!(format!("{msg:?}").contains("Disconnect"));
    }

    #[test]
    fn test_control_message_variants_are_distinct() {
        assert_ne!(ControlMessage::Keepalive, ControlMessage::KeepaliveAck);
        assert_ne!(ControlMessage::Keepalive, ControlMessage::Disconnect);
    }

    // ── SessionState ──────────────────────────────────────────────────────────

    #[test]
    fn test_session_state_connected_equals_connected() {
        assert_eq!(SessionState::Connected, SessionState::Connected);
    }

    #[test]
    fn test_session_state_variants_are_distinct() {
        assert_ne!(SessionState::Connected, SessionState::Reconnecting);
        assert_ne!(SessionState::Reconnecting, SessionState::Disconnected);
        assert_ne!(SessionState::Connected, SessionState::Disconnected);
    }

    #[test]
    fn test_session_state_clone() {
        let s = SessionState::Reconnecting;
        assert_eq!(s, s.clone());
    }

    #[test]
    fn test_session_state_copy() {
        let s = SessionState::Connected;
        let s2 = s;
        assert_eq!(s, s2);
    }

    #[test]
    fn test_session_state_debug() {
        assert!(format!("{:?}", SessionState::Connected).contains("Connected"));
        assert!(format!("{:?}", SessionState::Reconnecting).contains("Reconnecting"));
        assert!(format!("{:?}", SessionState::Disconnected).contains("Disconnected"));
    }

    // ── SessionError ──────────────────────────────────────────────────────────

    #[test]
    fn test_session_error_handshake_failed_display() {
        let e = SessionError::HandshakeFailed("bad message".to_string());
        assert_eq!(e.to_string(), "handshake failed: bad message");
    }

    #[test]
    fn test_session_error_keepalive_timeout_display() {
        let e = SessionError::KeepaliveTimeout;
        assert_eq!(e.to_string(), "keepalive timeout");
    }

    #[test]
    fn test_session_error_remote_closed_display() {
        let e = SessionError::RemoteClosed;
        assert_eq!(e.to_string(), "session closed by remote");
    }

    #[test]
    fn test_session_error_serialization_display() {
        let e = SessionError::Serialization("invalid json".to_string());
        assert_eq!(e.to_string(), "serialization error: invalid json");
    }

    #[test]
    fn test_session_error_transport_display() {
        let e = SessionError::Transport("connection lost".to_string());
        assert_eq!(e.to_string(), "transport error: connection lost");
    }

    #[test]
    fn test_session_error_debug_format() {
        let e = SessionError::KeepaliveTimeout;
        assert!(format!("{e:?}").contains("KeepaliveTimeout"));
    }
}
