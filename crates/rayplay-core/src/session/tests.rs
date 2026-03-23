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
    let b = StreamParams {
        width: 1280,
        ..a.clone()
    };
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

// ── ClientIntent ──────────────────────────────────────────────────────────

#[test]
fn test_client_intent_pair_serde_roundtrip() {
    let intent = ClientIntent::Pair;
    let json = serde_json::to_string(&intent).unwrap();
    let restored: ClientIntent = serde_json::from_str(&json).unwrap();
    assert_eq!(intent, restored);
}

#[test]
fn test_client_intent_auth_serde_roundtrip() {
    let intent = ClientIntent::Auth;
    let json = serde_json::to_string(&intent).unwrap();
    let restored: ClientIntent = serde_json::from_str(&json).unwrap();
    assert_eq!(intent, restored);
}

#[test]
fn test_client_intent_variants_are_distinct() {
    assert_ne!(ClientIntent::Pair, ClientIntent::Auth);
}

#[test]
fn test_client_intent_clone_equals_original() {
    let intent = ClientIntent::Pair;
    assert_eq!(intent.clone(), intent);
}

#[test]
fn test_client_intent_debug_format() {
    assert!(format!("{:?}", ClientIntent::Pair).contains("Pair"));
    assert!(format!("{:?}", ClientIntent::Auth).contains("Auth"));
}

// ── ControlMessage ──────────────────────────────────────────────────────────

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
fn test_serde_client_hello_pair() {
    let msg = ControlMessage::ClientHello(ClientIntent::Pair);
    let json = serde_json::to_string(&msg).unwrap();
    let restored: ControlMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(msg, restored);
}

#[test]
fn test_serde_client_hello_auth() {
    let msg = ControlMessage::ClientHello(ClientIntent::Auth);
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
    assert_ne!(
        ControlMessage::ClientHello(ClientIntent::Pair),
        ControlMessage::ClientHello(ClientIntent::Auth)
    );
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

#[test]
fn test_session_error_pairing_failed_display() {
    let e = SessionError::PairingFailed("wrong pin".to_string());
    assert_eq!(e.to_string(), "pairing failed: wrong pin");
}

// ── PairingOutcome ──────────────────────────────────────────────────────

#[test]
fn test_pairing_outcome_accepted_serde_roundtrip() {
    let o = PairingOutcome::Accepted;
    let json = serde_json::to_string(&o).unwrap();
    let restored: PairingOutcome = serde_json::from_str(&json).unwrap();
    assert_eq!(o, restored);
}

#[test]
fn test_pairing_outcome_rejected_serde_roundtrip() {
    let o = PairingOutcome::Rejected("wrong pin".to_string());
    let json = serde_json::to_string(&o).unwrap();
    let restored: PairingOutcome = serde_json::from_str(&json).unwrap();
    assert_eq!(o, restored);
}

#[test]
fn test_pairing_outcome_clone_equals_original() {
    let o = PairingOutcome::Accepted;
    assert_eq!(o.clone(), o);
}

#[test]
fn test_pairing_outcome_variants_are_distinct() {
    assert_ne!(
        PairingOutcome::Accepted,
        PairingOutcome::Rejected(String::new()),
    );
}

#[test]
fn test_pairing_outcome_debug_format() {
    assert!(format!("{:?}", PairingOutcome::Accepted).contains("Accepted"));
    assert!(format!("{:?}", PairingOutcome::Rejected("x".into())).contains("Rejected"));
}

// ── ControlMessage pairing variants serde ───────────────────────────────

#[test]
fn test_serde_pairing_request() {
    let msg = ControlMessage::PairingRequest(vec![1, 2, 3]);
    let json = serde_json::to_string(&msg).unwrap();
    let restored: ControlMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(msg, restored);
}

#[test]
fn test_serde_pairing_response() {
    let msg = ControlMessage::PairingResponse(vec![4, 5, 6]);
    let json = serde_json::to_string(&msg).unwrap();
    let restored: ControlMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(msg, restored);
}

#[test]
fn test_serde_pairing_confirm() {
    let msg = ControlMessage::PairingConfirm(vec![7, 8, 9]);
    let json = serde_json::to_string(&msg).unwrap();
    let restored: ControlMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(msg, restored);
}

#[test]
fn test_serde_pairing_result_accepted() {
    let msg = ControlMessage::PairingResult(PairingOutcome::Accepted);
    let json = serde_json::to_string(&msg).unwrap();
    let restored: ControlMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(msg, restored);
}

#[test]
fn test_serde_pairing_result_rejected() {
    let msg = ControlMessage::PairingResult(PairingOutcome::Rejected("bad".into()));
    let json = serde_json::to_string(&msg).unwrap();
    let restored: ControlMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(msg, restored);
}

#[test]
fn test_serde_auth_challenge() {
    let msg = ControlMessage::AuthChallenge(vec![10, 11]);
    let json = serde_json::to_string(&msg).unwrap();
    let restored: ControlMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(msg, restored);
}

#[test]
fn test_serde_auth_response() {
    let msg = ControlMessage::AuthResponse(vec![12, 13]);
    let json = serde_json::to_string(&msg).unwrap();
    let restored: ControlMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(msg, restored);
}

#[test]
fn test_serde_auth_result_true() {
    let msg = ControlMessage::AuthResult(true);
    let json = serde_json::to_string(&msg).unwrap();
    let restored: ControlMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(msg, restored);
}

#[test]
fn test_serde_auth_result_false() {
    let msg = ControlMessage::AuthResult(false);
    let json = serde_json::to_string(&msg).unwrap();
    let restored: ControlMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(msg, restored);
}

#[test]
fn test_pairing_variants_are_distinct() {
    assert_ne!(
        ControlMessage::PairingRequest(vec![1]),
        ControlMessage::PairingResponse(vec![1]),
    );
    assert_ne!(
        ControlMessage::AuthChallenge(vec![1]),
        ControlMessage::AuthResponse(vec![1]),
    );
}
