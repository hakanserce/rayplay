use super::*;
use crate::transport::QuicVideoTransport;
use std::net::SocketAddr;

async fn control_pair() -> (ControlChannel, ControlChannel) {
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (listener, cert_der) = QuicVideoTransport::listen(bind).unwrap();
    let server_addr = listener.local_addr().unwrap();

    let server_task = tokio::spawn(async move {
        let transport = listener.accept().await.expect("accept");
        transport.accept_control().await.expect("accept_control")
    });

    let client_transport = QuicVideoTransport::connect(server_addr, cert_der)
        .await
        .expect("connect");
    let mut client_ctrl = client_transport.open_control().await.expect("open_control");

    // QUIC only notifies the peer of a new bidi stream when a STREAM frame
    // is sent.  Send (and drain) a trigger keepalive so accept_bi fires.
    client_ctrl
        .sender
        .send(&ControlMessage::Keepalive)
        .await
        .expect("trigger");
    let mut server_ctrl = server_task.await.expect("server_task");
    let _ = server_ctrl.receiver.recv().await.expect("drain trigger");

    (client_ctrl, server_ctrl)
}

/// Consumes the `ClientHello` message and dispatches to the appropriate
/// host-side function (pairing or auth challenge), mirroring the CLI glue.
async fn host_dispatch(
    control: &mut ControlChannel,
    pin: &str,
    trust_db: &mut TrustDatabase,
    client_id: &str,
) -> Result<TrustedClient, SessionError> {
    let intent = match recv_msg(control, "hello").await? {
        ControlMessage::ClientHello(intent) => intent,
        other => {
            return Err(SessionError::PairingFailed(format!(
                "expected ClientHello, got {other:?}"
            )));
        }
    };
    match intent {
        ClientIntent::Pair => host_pairing(control, pin, trust_db, client_id).await,
        ClientIntent::Auth => host_auth_challenge(control, trust_db).await,
    }
}

// ── End-to-end pairing flows ─────────────────────────────────────────────

#[tokio::test]
async fn test_pairing_flow_success() {
    let (mut client_ctrl, mut server_ctrl) = control_pair().await;
    let pin = "123456";
    let mut trust_db = TrustDatabase::new();

    let (client_result, server_result) = tokio::join!(
        client_pairing(&mut client_ctrl, pin),
        host_dispatch(&mut server_ctrl, pin, &mut trust_db, "test-client"),
    );

    let client_key = client_result.unwrap();
    let trusted_client = server_result.unwrap();
    assert_eq!(trusted_client.client_id, "test-client");
    assert_eq!(trust_db.len(), 1);

    // Verify the public key matches
    let expected_pubkey = rayplay_core::pairing::encode_public_key(&client_key.verifying_key());
    assert_eq!(trusted_client.public_key, expected_pubkey);
}

#[tokio::test]
async fn test_pairing_flow_pin_mismatch() {
    let (mut client_ctrl, mut server_ctrl) = control_pair().await;
    let mut trust_db = TrustDatabase::new();

    let (client_result, server_result) = tokio::join!(
        client_pairing(&mut client_ctrl, "123456"),
        host_dispatch(&mut server_ctrl, "654321", &mut trust_db, "test-client"),
    );

    assert!(client_result.is_err());
    assert!(server_result.is_err());
    assert!(trust_db.is_empty());
}

// ── End-to-end auth flows ────────────────────────────────────────────────

#[tokio::test]
async fn test_auth_flow_success() {
    let (mut client_ctrl, mut server_ctrl) = control_pair().await;
    let mut trust_db = TrustDatabase::new();

    // First, pair to get a trusted key
    let pin = "123456";
    let (client_key, _) = tokio::join!(
        client_pairing(&mut client_ctrl, pin),
        host_dispatch(&mut server_ctrl, pin, &mut trust_db, "test-device"),
    );
    let key = client_key.unwrap();

    // Now test auth challenge
    let (mut client_ctrl2, mut server_ctrl2) = control_pair().await;
    let (client_result, server_result) = tokio::join!(
        client_auth_response(&mut client_ctrl2, &key),
        host_dispatch(&mut server_ctrl2, "", &mut trust_db, ""),
    );

    client_result.unwrap();
    let authenticated_client = server_result.unwrap();
    assert_eq!(authenticated_client.client_id, "test-device");
}

#[tokio::test]
async fn test_auth_challenge_untrusted_client() {
    let (mut client_ctrl, mut server_ctrl) = control_pair().await;
    let mut trust_db = TrustDatabase::new();
    let untrusted_key = SigningKey::generate(&mut rand_core::OsRng);

    let (client_result, server_result) = tokio::join!(
        client_auth_response(&mut client_ctrl, &untrusted_key),
        host_dispatch(&mut server_ctrl, "", &mut trust_db, ""),
    );

    assert!(client_result.is_err());
    assert!(server_result.is_err());
}

// ── Error conditions ─────────────────────────────────────────────────────

#[tokio::test]
async fn test_pairing_unexpected_message() {
    let (mut client_ctrl, mut server_ctrl) = control_pair().await;
    let mut trust_db = TrustDatabase::new();

    // Client sends wrong message type after ClientHello
    let server_task = tokio::spawn(async move {
        host_dispatch(&mut server_ctrl, "123456", &mut trust_db, "test").await
    });

    // Client sends ClientHello correctly
    send_msg(
        &mut client_ctrl,
        &ControlMessage::ClientHello(ClientIntent::Pair),
    )
    .await
    .unwrap();

    // But then sends wrong message
    send_msg(&mut client_ctrl, &ControlMessage::Keepalive)
        .await
        .unwrap();

    let result = server_task.await.unwrap();
    assert!(result.is_err());
}

#[tokio::test]
async fn test_auth_unexpected_message() {
    let (mut client_ctrl, mut server_ctrl) = control_pair().await;
    let mut trust_db = TrustDatabase::new();

    let server_task =
        tokio::spawn(async move { host_auth_challenge(&mut server_ctrl, &mut trust_db).await });

    // Client sends wrong message instead of AuthResponse
    let nonce = recv_msg(&mut client_ctrl, "test").await.unwrap();
    match nonce {
        ControlMessage::AuthChallenge(_) => {
            send_msg(&mut client_ctrl, &ControlMessage::Disconnect)
                .await
                .unwrap();
        }
        _ => panic!("Expected AuthChallenge"),
    }

    let result = server_task.await.unwrap();
    assert!(result.is_err());
}

#[tokio::test]
async fn test_pairing_invalid_confirm_length() {
    let (mut client_ctrl, mut server_ctrl) = control_pair().await;
    let mut trust_db = TrustDatabase::new();

    let server_task = tokio::spawn(async move {
        host_dispatch(&mut server_ctrl, "123456", &mut trust_db, "test").await
    });

    // Send valid ClientHello and PairingRequest
    send_msg(
        &mut client_ctrl,
        &ControlMessage::ClientHello(ClientIntent::Pair),
    )
    .await
    .unwrap();

    let (state, client_msg) = Spake2::<Ed25519Group>::start_a(
        &Password::new(b"123456"),
        &Identity::new(CLIENT_IDENTITY),
        &Identity::new(HOST_IDENTITY),
    );

    send_msg(
        &mut client_ctrl,
        &ControlMessage::PairingRequest(client_msg),
    )
    .await
    .unwrap();

    // Wait for PairingResponse
    let host_msg = recv_msg(&mut client_ctrl, "test").await.unwrap();
    if let ControlMessage::PairingResponse(msg) = host_msg {
        let _key = state.finish(&msg).unwrap();
        // Send invalid confirm (wrong length)
        send_msg(
            &mut client_ctrl,
            &ControlMessage::PairingConfirm(vec![1, 2, 3]),
        )
        .await
        .unwrap();
    } else {
        panic!("Expected PairingResponse");
    }

    let result = server_task.await.unwrap();
    assert!(result.is_err());
}

#[tokio::test]
async fn test_auth_invalid_response_length() {
    let (mut client_ctrl, mut server_ctrl) = control_pair().await;
    let mut trust_db = TrustDatabase::new();

    let server_task =
        tokio::spawn(async move { host_auth_challenge(&mut server_ctrl, &mut trust_db).await });

    // Wait for challenge and send invalid response
    let challenge = recv_msg(&mut client_ctrl, "test").await.unwrap();
    if let ControlMessage::AuthChallenge(_nonce) = challenge {
        // Send wrong length response
        send_msg(
            &mut client_ctrl,
            &ControlMessage::AuthResponse(vec![1, 2, 3]),
        )
        .await
        .unwrap();
    } else {
        panic!("Expected AuthChallenge");
    }

    let result = server_task.await.unwrap();
    assert!(result.is_err());
}

#[tokio::test]
async fn test_client_pairing_connection_closed_during_wait() {
    let (mut client_ctrl, mut server_ctrl) = control_pair().await;

    // Server closes connection before sending response
    let client_task = tokio::spawn(async move { client_pairing(&mut client_ctrl, "123456").await });

    // Send ClientHello, receive, then close
    let hello = recv_msg(&mut server_ctrl, "test").await.unwrap();
    if let ControlMessage::ClientHello(ClientIntent::Pair) = hello {
        // Receive PairingRequest but don't respond
        let _request = recv_msg(&mut server_ctrl, "test").await.unwrap();
        drop(server_ctrl);
    }

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let result = client_task.await.unwrap();
    assert!(result.is_err());
}

#[tokio::test]
async fn test_client_auth_connection_closed_during_wait() {
    let (mut client_ctrl, mut server_ctrl) = control_pair().await;

    // Server sends challenge but then closes
    let server_task = tokio::spawn(async move {
        // Just send AuthChallenge and close
        send_msg(
            &mut server_ctrl,
            &ControlMessage::AuthChallenge(vec![1; 32]),
        )
        .await
        .unwrap();
        drop(server_ctrl);
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let key = SigningKey::generate(&mut rand_core::OsRng);
    let result = client_auth_response(&mut client_ctrl, &key).await;
    assert!(result.is_err());
    server_task.await.unwrap();
}

#[tokio::test]
async fn test_client_pairing_unexpected_response() {
    let (mut client_ctrl, mut server_ctrl) = control_pair().await;

    let server_task = tokio::spawn(async move {
        // Wait for ClientHello
        let _hello = recv_msg(&mut server_ctrl, "test").await.unwrap();
        // Wait for PairingRequest but send wrong response
        let _request = recv_msg(&mut server_ctrl, "test").await.unwrap();
        send_msg(&mut server_ctrl, &ControlMessage::Keepalive)
            .await
            .unwrap();
    });

    let result = client_pairing(&mut client_ctrl, "123456").await;
    assert!(result.is_err());
    server_task.await.unwrap();
}

#[tokio::test]
async fn test_client_auth_unexpected_challenge_message() {
    let (mut client_ctrl, mut server_ctrl) = control_pair().await;

    let key = SigningKey::generate(&mut rand_core::OsRng);

    let server_task = tokio::spawn(async move {
        // Wait for ClientHello
        let _hello = recv_msg(&mut server_ctrl, "test").await.unwrap();
        // Send wrong message instead of AuthChallenge
        send_msg(&mut server_ctrl, &ControlMessage::Keepalive)
            .await
            .unwrap();
    });

    let result = client_auth_response(&mut client_ctrl, &key).await;
    assert!(result.is_err());
    server_task.await.unwrap();
}

#[tokio::test]
async fn test_client_auth_unexpected_result_message() {
    let (mut client_ctrl, mut server_ctrl) = control_pair().await;

    let key = SigningKey::generate(&mut rand_core::OsRng);

    let server_task = tokio::spawn(async move {
        // Wait for ClientHello
        let _hello = recv_msg(&mut server_ctrl, "test").await.unwrap();
        // Send AuthChallenge
        send_msg(
            &mut server_ctrl,
            &ControlMessage::AuthChallenge(vec![0; 32]),
        )
        .await
        .unwrap();
        // Wait for AuthResponse
        let _response = recv_msg(&mut server_ctrl, "test").await.unwrap();
        // Send wrong message instead of AuthResult
        send_msg(&mut server_ctrl, &ControlMessage::Keepalive)
            .await
            .unwrap();
    });

    let result = client_auth_response(&mut client_ctrl, &key).await;
    assert!(result.is_err());
    server_task.await.unwrap();
}

#[tokio::test]
async fn test_client_pairing_stream_closed_unexpectedly() {
    let (mut client_ctrl, mut server_ctrl) = control_pair().await;

    let server_task = tokio::spawn(async move {
        // Wait for ClientHello and PairingRequest, then close stream
        let _hello = recv_msg(&mut server_ctrl, "test").await.unwrap();
        let _request = recv_msg(&mut server_ctrl, "test").await.unwrap();
        server_ctrl.sender.stream.finish().unwrap();
    });

    let result = client_pairing(&mut client_ctrl, "123456").await;
    assert!(result.is_err());
    server_task.await.unwrap();
}

#[tokio::test]
async fn test_client_auth_stream_closed_unexpectedly() {
    let (mut client_ctrl, mut server_ctrl) = control_pair().await;

    let server_task = tokio::spawn(async move {
        // Wait for ClientHello, send challenge, wait for response, then close
        let _hello = recv_msg(&mut server_ctrl, "test").await.unwrap();
        send_msg(
            &mut server_ctrl,
            &ControlMessage::AuthChallenge(vec![0; 32]),
        )
        .await
        .unwrap();
        let _response = recv_msg(&mut server_ctrl, "test").await.unwrap();
        server_ctrl.sender.stream.finish().unwrap();
    });

    let key = SigningKey::generate(&mut rand_core::OsRng);
    let result = client_auth_response(&mut client_ctrl, &key).await;
    assert!(result.is_err());
    server_task.await.unwrap();
}

// ── Auth edge-case error paths ─────────────────────────────────────────

#[tokio::test]
async fn test_auth_invalid_signature_rejected() {
    // Pair first to get a trusted key
    let (mut client_ctrl, mut server_ctrl) = control_pair().await;
    let pin = "123456";
    let mut trust_db = TrustDatabase::new();

    let (client_key, _) = tokio::join!(
        client_pairing(&mut client_ctrl, pin),
        host_dispatch(&mut server_ctrl, pin, &mut trust_db, "test-device"),
    );
    let key = client_key.unwrap();

    // Now manually craft an auth response with a bad signature
    let (mut client_ctrl2, mut server_ctrl2) = control_pair().await;
    let server_task =
        tokio::spawn(async move { host_auth_challenge(&mut server_ctrl2, &mut trust_db).await });

    // Wait for challenge
    let challenge = recv_msg(&mut client_ctrl2, "test").await.unwrap();
    let _nonce = match challenge {
        ControlMessage::AuthChallenge(n) => n,
        _ => panic!("Expected AuthChallenge"),
    };

    // Send response with correct pubkey but wrong signature (all zeros)
    let mut payload = Vec::with_capacity(96);
    payload.extend_from_slice(key.verifying_key().as_bytes());
    payload.extend_from_slice(&[0u8; 64]); // invalid signature
    send_msg(&mut client_ctrl2, &ControlMessage::AuthResponse(payload))
        .await
        .unwrap();

    // Drain AuthResult to avoid broken pipe
    let _ = recv_msg(&mut client_ctrl2, "drain").await;

    let result = server_task.await.unwrap();
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("signature verification failed"),
        "unexpected error: {err_msg}"
    );
}

#[tokio::test]
async fn test_auth_invalid_curve_point_rejected() {
    let (mut client_ctrl, mut server_ctrl) = control_pair().await;
    let mut trust_db = TrustDatabase::new();

    let server_task =
        tokio::spawn(async move { host_auth_challenge(&mut server_ctrl, &mut trust_db).await });

    // Wait for challenge
    let challenge = recv_msg(&mut client_ctrl, "test").await.unwrap();
    assert!(matches!(challenge, ControlMessage::AuthChallenge(_)));

    // Send response with invalid curve point (y=2) as public key + dummy signature
    let mut bad_pubkey = [0u8; 32];
    bad_pubkey[0] = 2; // Not on the Ed25519 curve
    let mut payload = Vec::with_capacity(96);
    payload.extend_from_slice(&bad_pubkey);
    payload.extend_from_slice(&[0u8; 64]);
    send_msg(&mut client_ctrl, &ControlMessage::AuthResponse(payload))
        .await
        .unwrap();

    let result = server_task.await.unwrap();
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("invalid public key"),
        "unexpected error: {err_msg}"
    );
}

#[tokio::test]
async fn test_pairing_confirm_invalid_curve_point() {
    let (mut client_ctrl, mut server_ctrl) = control_pair().await;
    let mut trust_db = TrustDatabase::new();
    let pin = "123456";

    let server_task =
        tokio::spawn(
            async move { host_dispatch(&mut server_ctrl, pin, &mut trust_db, "test").await },
        );

    // Send ClientHello + PairingRequest
    send_msg(
        &mut client_ctrl,
        &ControlMessage::ClientHello(ClientIntent::Pair),
    )
    .await
    .unwrap();

    let (state, client_msg) = Spake2::<Ed25519Group>::start_a(
        &Password::new(pin.as_bytes()),
        &Identity::new(CLIENT_IDENTITY),
        &Identity::new(HOST_IDENTITY),
    );

    send_msg(
        &mut client_ctrl,
        &ControlMessage::PairingRequest(client_msg),
    )
    .await
    .unwrap();

    // Wait for PairingResponse
    let host_msg = recv_msg(&mut client_ctrl, "test").await.unwrap();
    if let ControlMessage::PairingResponse(msg) = host_msg {
        let key = state.finish(&msg).unwrap();

        // Send confirm with invalid curve point (y=2) but valid HMAC
        let mut bad_pubkey = [0u8; 32];
        bad_pubkey[0] = 2; // Not on Ed25519 curve
        let hmac = simple_hmac(&key, &bad_pubkey);
        let mut payload = Vec::with_capacity(64);
        payload.extend_from_slice(&bad_pubkey);
        payload.extend_from_slice(&hmac);
        send_msg(&mut client_ctrl, &ControlMessage::PairingConfirm(payload))
            .await
            .unwrap();
    } else {
        panic!("Expected PairingResponse");
    }

    let result = server_task.await.unwrap();
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("invalid public key"),
        "unexpected error: {err_msg}"
    );
}

#[tokio::test]
async fn test_pairing_confirm_unexpected_message() {
    let (mut client_ctrl, mut server_ctrl) = control_pair().await;
    let mut trust_db = TrustDatabase::new();
    let pin = "123456";

    let server_task =
        tokio::spawn(
            async move { host_dispatch(&mut server_ctrl, pin, &mut trust_db, "test").await },
        );

    // Send ClientHello + PairingRequest
    send_msg(
        &mut client_ctrl,
        &ControlMessage::ClientHello(ClientIntent::Pair),
    )
    .await
    .unwrap();

    let (_state, client_msg) = Spake2::<Ed25519Group>::start_a(
        &Password::new(pin.as_bytes()),
        &Identity::new(CLIENT_IDENTITY),
        &Identity::new(HOST_IDENTITY),
    );

    send_msg(
        &mut client_ctrl,
        &ControlMessage::PairingRequest(client_msg),
    )
    .await
    .unwrap();

    // Wait for PairingResponse, then send wrong message instead of PairingConfirm
    let _host_msg = recv_msg(&mut client_ctrl, "test").await.unwrap();
    send_msg(&mut client_ctrl, &ControlMessage::Keepalive)
        .await
        .unwrap();

    let result = server_task.await.unwrap();
    assert!(result.is_err());
}

#[tokio::test]
async fn test_client_pairing_result_unexpected_message() {
    let (mut client_ctrl, mut server_ctrl) = control_pair().await;
    let pin = "123456";

    let server_task = tokio::spawn(async move {
        // Consume ClientHello
        let _hello = recv_msg(&mut server_ctrl, "test").await.unwrap();
        // Consume PairingRequest
        let _request = recv_msg(&mut server_ctrl, "test").await.unwrap();
        // Send PairingResponse (dummy SPAKE2 message)
        let (_state, host_msg) = Spake2::<Ed25519Group>::start_b(
            &Password::new(pin.as_bytes()),
            &Identity::new(CLIENT_IDENTITY),
            &Identity::new(HOST_IDENTITY),
        );
        send_msg(&mut server_ctrl, &ControlMessage::PairingResponse(host_msg))
            .await
            .unwrap();
        // Wait for PairingConfirm, then send wrong message instead of PairingResult
        let _confirm = recv_msg(&mut server_ctrl, "test").await.unwrap();
        send_msg(&mut server_ctrl, &ControlMessage::Keepalive)
            .await
            .unwrap();
    });

    let result = client_pairing(&mut client_ctrl, pin).await;
    assert!(result.is_err());
    server_task.await.unwrap();
}

#[tokio::test]
async fn test_client_auth_rejected() {
    let (mut client_ctrl, mut server_ctrl) = control_pair().await;
    let key = SigningKey::generate(&mut rand_core::OsRng);

    let server_task = tokio::spawn(async move {
        // Wait for ClientHello
        let _hello = recv_msg(&mut server_ctrl, "test").await.unwrap();
        // Send challenge
        send_msg(
            &mut server_ctrl,
            &ControlMessage::AuthChallenge(vec![0; 32]),
        )
        .await
        .unwrap();
        // Wait for response
        let _response = recv_msg(&mut server_ctrl, "test").await.unwrap();
        // Send rejection
        send_msg(&mut server_ctrl, &ControlMessage::AuthResult(false))
            .await
            .unwrap();
        // Keep the connection alive until client reads the result
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        drop(server_ctrl);
    });

    let result = client_auth_response(&mut client_ctrl, &key).await;
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("authentication rejected"),
        "unexpected error: {err_msg}"
    );
    server_task.await.unwrap();
}

// ── SPAKE2 finish() failure tests ────────────────────────────────────────

#[tokio::test]
async fn test_host_pairing_spake2_finish_fails_with_wrong_identity() {
    let (mut client_ctrl, mut server_ctrl) = control_pair().await;
    let mut trust_db = TrustDatabase::new();
    let pin = "123456";

    let server_task =
        tokio::spawn(
            async move { host_dispatch(&mut server_ctrl, pin, &mut trust_db, "test").await },
        );

    // Send valid ClientHello
    send_msg(
        &mut client_ctrl,
        &ControlMessage::ClientHello(ClientIntent::Pair),
    )
    .await
    .unwrap();

    // Start SPAKE2 with WRONG identities (swapped)
    let (_state, client_msg) = Spake2::<Ed25519Group>::start_a(
        &Password::new(pin.as_bytes()),
        &Identity::new(HOST_IDENTITY), // WRONG: should be CLIENT_IDENTITY
        &Identity::new(CLIENT_IDENTITY), // WRONG: should be HOST_IDENTITY
    );

    send_msg(
        &mut client_ctrl,
        &ControlMessage::PairingRequest(client_msg),
    )
    .await
    .unwrap();

    let result = server_task.await.unwrap();
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    // Allow either SPAKE2 finish failed or connection lost error
    assert!(
        err_msg.contains("SPAKE2 finish failed") || err_msg.contains("connection lost"),
        "unexpected error: {err_msg}"
    );
}

#[tokio::test]
async fn test_client_pairing_spake2_finish_fails_with_garbage_message() {
    let (mut client_ctrl, mut server_ctrl) = control_pair().await;
    let pin = "123456";

    let server_task = tokio::spawn(async move {
        // Wait for ClientHello
        let _hello = recv_msg(&mut server_ctrl, "test").await.unwrap();
        // Wait for PairingRequest
        let _request = recv_msg(&mut server_ctrl, "test").await.unwrap();
        // Send garbage as PairingResponse instead of valid SPAKE2 message
        let garbage_msg = vec![1, 2, 3, 4, 5];
        send_msg(
            &mut server_ctrl,
            &ControlMessage::PairingResponse(garbage_msg),
        )
        .await
        .unwrap();
    });

    let result = client_pairing(&mut client_ctrl, pin).await;
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    // Allow either SPAKE2 finish failed or connection lost error
    assert!(
        err_msg.contains("SPAKE2 finish failed") || err_msg.contains("connection lost"),
        "unexpected error: {err_msg}"
    );
    server_task.await.unwrap();
}

#[tokio::test]
async fn test_host_pairing_spake2_finish_fails_with_garbage_message() {
    let (mut client_ctrl, mut server_ctrl) = control_pair().await;
    let mut trust_db = TrustDatabase::new();
    let pin = "123456";

    let server_task =
        tokio::spawn(
            async move { host_dispatch(&mut server_ctrl, pin, &mut trust_db, "test").await },
        );

    // Send valid ClientHello
    send_msg(
        &mut client_ctrl,
        &ControlMessage::ClientHello(ClientIntent::Pair),
    )
    .await
    .unwrap();

    // Send garbage as PairingRequest instead of valid SPAKE2 message
    let garbage_msg = vec![42; 33]; // Random 33 bytes to trigger failure
    send_msg(
        &mut client_ctrl,
        &ControlMessage::PairingRequest(garbage_msg),
    )
    .await
    .unwrap();

    let result = server_task.await.unwrap();
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("SPAKE2 finish failed"),
        "unexpected error: {err_msg}"
    );
}

// ── simple_hmac ─────────────────────────────────────────────────────────

#[test]
fn test_simple_hmac_deterministic() {
    let key = b"test-key";
    let data = b"test-data";
    let h1 = simple_hmac(key, data);
    let h2 = simple_hmac(key, data);
    assert_eq!(h1, h2);
}

#[test]
fn test_simple_hmac_different_keys_differ() {
    let data = b"test-data";
    let h1 = simple_hmac(b"key1", data);
    let h2 = simple_hmac(b"key2", data);
    assert_ne!(h1, h2);
}

#[test]
fn test_simple_hmac_different_data_differ() {
    let key = b"test-key";
    let h1 = simple_hmac(key, b"data1");
    let h2 = simple_hmac(key, b"data2");
    assert_ne!(h1, h2);
}

#[test]
fn test_simple_hmac_length() {
    let h = simple_hmac(b"key", b"data");
    assert_eq!(h.len(), 32);
}
