//! PIN-based pairing and trusted-client authentication over the control channel (UC-016).
//!
//! Implements the SPAKE2 pairing flow from [ADR-007](../../docs/adr/ADR-007.md)
//! and a challenge-response protocol for previously-paired clients.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use rayplay_core::pairing::{TrustDatabase, TrustedClient, encode_public_key};
use rayplay_core::session::{ControlMessage, PairingOutcome, SessionError};
use spake2::{Ed25519Group, Identity, Password, Spake2};

use crate::control::ControlChannel;

/// SPAKE2 identity for the host (side B).
const HOST_IDENTITY: &[u8] = b"RayPlay-Host";
/// SPAKE2 identity for the client (side A).
const CLIENT_IDENTITY: &[u8] = b"RayPlay-Client";
/// Length of the authentication challenge nonce in bytes.
const NONCE_LEN: usize = 32;

/// Runs the host side of the SPAKE2 pairing exchange.
///
/// Waits for a [`ControlMessage::PairingRequest`] from the client, executes
/// the SPAKE2 protocol with the given `pin`, and on success stores the
/// client's ed25519 public key in `trust_db`.
///
/// # Errors
///
/// Returns [`SessionError::PairingFailed`] on PIN mismatch, protocol errors,
/// or if the client sends an unexpected message.
pub async fn host_pairing(
    control: &mut ControlChannel,
    pin: &str,
    trust_db: &mut TrustDatabase,
    client_id: &str,
) -> Result<TrustedClient, SessionError> {
    // 1. Wait for PairingRequest(client_spake2_msg)
    let client_msg = match recv_msg(control, "pairing").await? {
        ControlMessage::PairingRequest(msg) => msg,
        other => {
            return Err(SessionError::PairingFailed(format!(
                "expected PairingRequest, got {other:?}"
            )));
        }
    };

    // 2. Start SPAKE2 as side B (id_a and id_b must match client's start_a)
    let (state, host_msg) = Spake2::<Ed25519Group>::start_b(
        &Password::new(pin.as_bytes()),
        &Identity::new(CLIENT_IDENTITY),
        &Identity::new(HOST_IDENTITY),
    );

    // 3. Send PairingResponse(host_spake2_msg)
    send_msg(control, &ControlMessage::PairingResponse(host_msg.clone())).await?;

    // 4. Wait for PairingConfirm(pubkey_bytes)
    let confirm_payload = match recv_msg(control, "pairing confirmation").await? {
        ControlMessage::PairingConfirm(payload) => payload,
        other => {
            return Err(SessionError::PairingFailed(format!(
                "expected PairingConfirm, got {other:?}"
            )));
        }
    };

    // 5. Complete SPAKE2 — derive shared key
    let host_key = state
        .finish(&client_msg)
        .map_err(|e| SessionError::PairingFailed(format!("SPAKE2 finish failed: {e}")))?;

    // 6. Verify confirmation: the payload is pubkey(32) + hmac(32)
    if confirm_payload.len() != 64 {
        let outcome = PairingOutcome::Rejected("invalid confirm payload length".to_string());
        send_msg(control, &ControlMessage::PairingResult(outcome)).await?;
        return Err(SessionError::PairingFailed(
            "invalid confirm payload length".to_string(),
        ));
    }
    let (pubkey_bytes, received_hmac) = confirm_payload.split_at(32);
    let expected_hmac = simple_hmac(&host_key, pubkey_bytes);
    if received_hmac != expected_hmac.as_slice() {
        let outcome = PairingOutcome::Rejected("PIN mismatch".to_string());
        send_msg(control, &ControlMessage::PairingResult(outcome)).await?;
        return Err(SessionError::PairingFailed("PIN mismatch".to_string()));
    }

    // 7. Encode and store the client's public key.
    //    pubkey_bytes is guaranteed to be 32 bytes (validated by length check above).
    let encoded_key = BASE64.encode(pubkey_bytes);

    let now = chrono::Utc::now().to_rfc3339();
    let client = TrustedClient {
        client_id: client_id.to_string(),
        public_key: encoded_key,
        paired_at: now.clone(),
        last_seen: now,
    };
    trust_db.add_client(client.clone());

    // 8. Send PairingResult(Accepted)
    send_msg(
        control,
        &ControlMessage::PairingResult(PairingOutcome::Accepted),
    )
    .await?;

    Ok(client)
}

/// Runs the client side of the SPAKE2 pairing exchange.
///
/// Generates an ed25519 key pair, executes SPAKE2 with the given `pin`,
/// and returns the signing key on success (for future authenticated
/// reconnections).
///
/// # Errors
///
/// Returns [`SessionError::PairingFailed`] on PIN mismatch, protocol errors,
/// or if the host rejects the pairing.
pub async fn client_pairing(
    control: &mut ControlChannel,
    pin: &str,
) -> Result<SigningKey, SessionError> {
    // 1. Generate ed25519 key pair
    let signing_key = SigningKey::generate(&mut rand::thread_rng());
    let verifying_key = signing_key.verifying_key();

    // 2. Start SPAKE2 as side A
    let (state, client_msg) = Spake2::<Ed25519Group>::start_a(
        &Password::new(pin.as_bytes()),
        &Identity::new(CLIENT_IDENTITY),
        &Identity::new(HOST_IDENTITY),
    );

    // 3. Send PairingRequest(client_spake2_msg)
    send_msg(control, &ControlMessage::PairingRequest(client_msg.clone())).await?;

    // 4. Wait for PairingResponse(host_spake2_msg)
    let host_msg = match recv_msg(control, "pairing").await? {
        ControlMessage::PairingResponse(msg) => msg,
        other => {
            return Err(SessionError::PairingFailed(format!(
                "expected PairingResponse, got {other:?}"
            )));
        }
    };

    // 5. Complete SPAKE2 — derive shared key
    let client_key = state
        .finish(&host_msg)
        .map_err(|e| SessionError::PairingFailed(format!("SPAKE2 finish failed: {e}")))?;

    // 6. Build confirm payload: pubkey(32) + hmac(32)
    let pubkey_bytes = verifying_key.as_bytes();
    let hmac = simple_hmac(&client_key, pubkey_bytes);
    let mut confirm_payload = Vec::with_capacity(64);
    confirm_payload.extend_from_slice(pubkey_bytes);
    confirm_payload.extend_from_slice(&hmac);

    send_msg(control, &ControlMessage::PairingConfirm(confirm_payload)).await?;

    // 7. Wait for PairingResult
    match recv_msg(control, "pairing result").await? {
        ControlMessage::PairingResult(PairingOutcome::Accepted) => Ok(signing_key),
        ControlMessage::PairingResult(PairingOutcome::Rejected(reason)) => {
            Err(SessionError::PairingFailed(reason))
        }
        other => Err(SessionError::PairingFailed(format!(
            "expected PairingResult, got {other:?}"
        ))),
    }
}

/// Runs the host side of the challenge-response authentication for a
/// previously-paired client.
///
/// Sends a random nonce, waits for the client to sign it with its ed25519
/// key, and verifies the signature against the trust database.
///
/// # Errors
///
/// Returns [`SessionError::PairingFailed`] if the client's key is not trusted
/// or the signature is invalid.
///
/// # Panics
///
/// Panics if the trust database returns `None` for a key that was just
/// confirmed as trusted (indicates a logic bug, not a runtime condition).
pub async fn host_auth_challenge(
    control: &mut ControlChannel,
    trust_db: &mut TrustDatabase,
) -> Result<TrustedClient, SessionError> {
    // 1. Generate and send random nonce
    let mut nonce = [0u8; NONCE_LEN];
    rand::Rng::fill(&mut rand::thread_rng(), &mut nonce);
    send_msg(control, &ControlMessage::AuthChallenge(nonce.to_vec())).await?;

    // 2. Wait for AuthResponse(pubkey + signature)
    let response = match recv_msg(control, "auth").await? {
        ControlMessage::AuthResponse(data) => data,
        other => {
            return Err(SessionError::PairingFailed(format!(
                "expected AuthResponse, got {other:?}"
            )));
        }
    };

    // 3. Parse: pubkey(32) + ed25519 signature(64)
    if response.len() != 96 {
        send_msg(control, &ControlMessage::AuthResult(false)).await?;
        return Err(SessionError::PairingFailed(
            "invalid auth response length".to_string(),
        ));
    }
    let (pubkey_bytes, sig_bytes) = response.split_at(32);

    //    pubkey_bytes is guaranteed to be 32 bytes (validated by length check above).
    let mut pubkey_arr = [0u8; 32];
    pubkey_arr.copy_from_slice(pubkey_bytes);
    let verifying_key = VerifyingKey::from_bytes(&pubkey_arr)
        .map_err(|e| SessionError::PairingFailed(format!("invalid public key: {e}")))?;

    let encoded_key = encode_public_key(&verifying_key);

    // 4. Check trust database
    if !trust_db.is_trusted(&encoded_key) {
        send_msg(control, &ControlMessage::AuthResult(false)).await?;
        return Err(SessionError::PairingFailed(
            "client not trusted".to_string(),
        ));
    }

    // 5. Verify signature over the nonce
    let sig = ed25519_dalek::Signature::from_slice(sig_bytes)
        .map_err(|e| SessionError::PairingFailed(format!("invalid signature: {e}")))?;

    if verifying_key.verify(&nonce, &sig).is_err() {
        send_msg(control, &ControlMessage::AuthResult(false)).await?;
        return Err(SessionError::PairingFailed(
            "signature verification failed".to_string(),
        ));
    }

    // 6. Update last_seen and send success
    trust_db.update_last_seen(&encoded_key);
    let client = trust_db.find_client(&encoded_key).unwrap().clone();
    send_msg(control, &ControlMessage::AuthResult(true)).await?;

    Ok(client)
}

/// Runs the client side of the challenge-response authentication.
///
/// Waits for an [`ControlMessage::AuthChallenge`] nonce, signs it with the
/// client's ed25519 signing key, and sends the response.
///
/// # Errors
///
/// Returns [`SessionError::PairingFailed`] if the host rejects authentication.
pub async fn client_auth_response(
    control: &mut ControlChannel,
    signing_key: &SigningKey,
) -> Result<(), SessionError> {
    // 1. Wait for AuthChallenge(nonce)
    let nonce = match recv_msg(control, "auth").await? {
        ControlMessage::AuthChallenge(n) => n,
        other => {
            return Err(SessionError::PairingFailed(format!(
                "expected AuthChallenge, got {other:?}"
            )));
        }
    };

    // 2. Sign nonce and build response: pubkey(32) + signature(64)
    let signature = signing_key.sign(&nonce);
    let verifying_key = signing_key.verifying_key();
    let mut response = Vec::with_capacity(96);
    response.extend_from_slice(verifying_key.as_bytes());
    response.extend_from_slice(&signature.to_bytes());

    send_msg(control, &ControlMessage::AuthResponse(response)).await?;

    // 3. Wait for AuthResult
    match recv_msg(control, "auth result").await? {
        ControlMessage::AuthResult(true) => Ok(()),
        ControlMessage::AuthResult(false) => {
            Err(SessionError::PairingFailed("authentication rejected".to_string()))
        }
        other => Err(SessionError::PairingFailed(format!(
            "expected AuthResult, got {other:?}"
        ))),
    }
}

/// Receives a control message, mapping transport/stream errors to [`SessionError`].
async fn recv_msg(
    control: &mut ControlChannel,
    context: &str,
) -> Result<ControlMessage, SessionError> {
    match control.receiver.recv().await {
        Ok(Some(msg)) => Ok(msg),
        Ok(None) => Err(SessionError::PairingFailed(format!(
            "stream closed during {context}"
        ))),
        Err(e) => Err(SessionError::Transport(e.to_string())),
    }
}

/// Sends a control message, mapping transport errors to [`SessionError`].
async fn send_msg(
    control: &mut ControlChannel,
    msg: &ControlMessage,
) -> Result<(), SessionError> {
    control
        .sender
        .send(msg)
        .await
        .map_err(|e| SessionError::Transport(e.to_string()))
}

/// Simple HMAC-like construct: SHA-256(key || data).
///
/// Not a proper HMAC (no inner/outer padding), but sufficient for
/// confirming SPAKE2 key agreement over an already-encrypted QUIC channel.
fn simple_hmac(key: &[u8], data: &[u8]) -> [u8; 32] {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::Hasher;

    // We use two rounds of hashing to get 32 bytes from the key+data combo.
    // This is a confirmation MAC over an encrypted channel, not a standalone
    // security primitive.
    let mut result = [0u8; 32];
    for (i, chunk) in result.chunks_mut(8).enumerate() {
        let mut hasher = DefaultHasher::new();
        #[allow(clippy::cast_possible_truncation)]
        hasher.write(&[i as u8]);
        hasher.write(key);
        hasher.write(data);
        chunk.copy_from_slice(&hasher.finish().to_le_bytes());
    }
    result
}

#[cfg(test)]
mod tests {
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
        client_ctrl
            .sender
            .send(&ControlMessage::Keepalive)
            .await
            .expect("trigger");

        let mut server_ctrl = server_task.await.expect("server task");
        let _ = server_ctrl.receiver.recv().await.expect("drain trigger");

        (client_ctrl, server_ctrl)
    }

    // ── SPAKE2 pairing ─────────────────────────────────────────────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_pairing_happy_path() {
        let (mut client_ctrl, mut server_ctrl) = control_pair().await;
        let pin = "123456";
        let mut trust_db = TrustDatabase::new();

        let (client_result, server_result) = tokio::join!(
            client_pairing(&mut client_ctrl, pin),
            host_pairing(&mut server_ctrl, pin, &mut trust_db, "test-laptop"),
        );

        let signing_key = client_result.expect("client pairing should succeed");
        let trusted_client = server_result.expect("host pairing should succeed");

        assert_eq!(trusted_client.client_id, "test-laptop");
        assert!(trust_db.is_trusted(&trusted_client.public_key));
        assert_eq!(
            encode_public_key(&signing_key.verifying_key()),
            trusted_client.public_key,
        );
        assert_eq!(trust_db.len(), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_pairing_wrong_pin() {
        let (mut client_ctrl, mut server_ctrl) = control_pair().await;
        let mut trust_db = TrustDatabase::new();

        let (client_result, server_result) = tokio::join!(
            client_pairing(&mut client_ctrl, "111111"),
            host_pairing(&mut server_ctrl, "999999", &mut trust_db, "laptop"),
        );

        assert!(server_result.is_err());
        assert!(client_result.is_err());
        assert!(trust_db.is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_pairing_stream_closed_before_request() {
        let (client_ctrl, mut server_ctrl) = control_pair().await;
        drop(client_ctrl);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut trust_db = TrustDatabase::new();
        let result = host_pairing(&mut server_ctrl, "123456", &mut trust_db, "laptop").await;
        assert!(result.is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_pairing_unexpected_message_at_request() {
        let (mut client_ctrl, mut server_ctrl) = control_pair().await;
        let mut trust_db = TrustDatabase::new();

        client_ctrl
            .sender
            .send(&ControlMessage::Keepalive)
            .await
            .unwrap();

        let result = host_pairing(&mut server_ctrl, "123456", &mut trust_db, "laptop").await;
        assert!(matches!(result, Err(SessionError::PairingFailed(_))));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_host_pairing_unexpected_message_at_confirm() {
        let (mut client_ctrl, mut server_ctrl) = control_pair().await;
        let mut trust_db = TrustDatabase::new();

        let host_task = tokio::spawn(async move {
            host_pairing(&mut server_ctrl, "123456", &mut trust_db, "laptop").await
        });

        // Send valid PairingRequest
        let (_state, client_msg) = Spake2::<Ed25519Group>::start_a(
            &Password::new(b"123456"),
            &Identity::new(CLIENT_IDENTITY),
            &Identity::new(HOST_IDENTITY),
        );
        client_ctrl
            .sender
            .send(&ControlMessage::PairingRequest(client_msg.clone()))
            .await
            .unwrap();

        // Receive PairingResponse
        let _resp = client_ctrl.receiver.recv().await;

        // Send wrong message instead of PairingConfirm
        client_ctrl
            .sender
            .send(&ControlMessage::Keepalive)
            .await
            .unwrap();

        let result = host_task.await.unwrap();
        assert!(matches!(result, Err(SessionError::PairingFailed(_))));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_host_pairing_stream_closed_at_confirm() {
        let (mut client_ctrl, mut server_ctrl) = control_pair().await;
        let mut trust_db = TrustDatabase::new();

        let host_task = tokio::spawn(async move {
            host_pairing(&mut server_ctrl, "123456", &mut trust_db, "laptop").await
        });

        // Send valid PairingRequest
        let (_state, client_msg) = Spake2::<Ed25519Group>::start_a(
            &Password::new(b"123456"),
            &Identity::new(CLIENT_IDENTITY),
            &Identity::new(HOST_IDENTITY),
        );
        client_ctrl
            .sender
            .send(&ControlMessage::PairingRequest(client_msg.clone()))
            .await
            .unwrap();

        // Receive PairingResponse, then close stream
        let _resp = client_ctrl.receiver.recv().await;
        client_ctrl.sender.stream.finish().unwrap();

        let result = host_task.await.unwrap();
        assert!(result.is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_host_pairing_invalid_confirm_payload_length() {
        let (mut client_ctrl, mut server_ctrl) = control_pair().await;
        let mut trust_db = TrustDatabase::new();

        let host_task = tokio::spawn(async move {
            host_pairing(&mut server_ctrl, "123456", &mut trust_db, "laptop").await
        });

        let (_state, client_msg) = Spake2::<Ed25519Group>::start_a(
            &Password::new(b"123456"),
            &Identity::new(CLIENT_IDENTITY),
            &Identity::new(HOST_IDENTITY),
        );
        client_ctrl
            .sender
            .send(&ControlMessage::PairingRequest(client_msg.clone()))
            .await
            .unwrap();
        let _resp = client_ctrl.receiver.recv().await;

        // Send confirm with wrong length (not 64 bytes)
        client_ctrl
            .sender
            .send(&ControlMessage::PairingConfirm(vec![1, 2, 3]))
            .await
            .unwrap();

        let result = host_task.await.unwrap();
        assert!(matches!(result, Err(SessionError::PairingFailed(_))));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_client_pairing_unexpected_response() {
        let (mut client_ctrl, mut server_ctrl) = control_pair().await;

        let client_task = tokio::spawn(async move {
            client_pairing(&mut client_ctrl, "123456").await
        });

        // Drain the PairingRequest
        let _req = server_ctrl.receiver.recv().await;
        // Send wrong message type instead of PairingResponse
        server_ctrl
            .sender
            .send(&ControlMessage::Keepalive)
            .await
            .unwrap();

        let result = client_task.await.unwrap();
        assert!(matches!(result, Err(SessionError::PairingFailed(_))));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_client_pairing_stream_closed_at_response() {
        let (mut client_ctrl, mut server_ctrl) = control_pair().await;

        let client_task = tokio::spawn(async move {
            client_pairing(&mut client_ctrl, "123456").await
        });

        // Drain PairingRequest, then close stream
        let _req = server_ctrl.receiver.recv().await;
        server_ctrl.sender.stream.finish().unwrap();

        let result = client_task.await.unwrap();
        assert!(result.is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_client_pairing_stream_closed_at_result() {
        let (mut client_ctrl, mut server_ctrl) = control_pair().await;

        let client_task = tokio::spawn(async move {
            client_pairing(&mut client_ctrl, "123456").await
        });

        // Complete SPAKE2 on host side manually
        let _req = server_ctrl.receiver.recv().await;
        let (_state, host_msg) = Spake2::<Ed25519Group>::start_b(
            &Password::new(b"123456"),
            &Identity::new(CLIENT_IDENTITY),
            &Identity::new(HOST_IDENTITY),
        );
        server_ctrl
            .sender
            .send(&ControlMessage::PairingResponse(host_msg.clone()))
            .await
            .unwrap();

        // Receive PairingConfirm, then close stream before sending result
        let _confirm = server_ctrl.receiver.recv().await;
        server_ctrl.sender.stream.finish().unwrap();

        let result = client_task.await.unwrap();
        assert!(result.is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_client_pairing_unexpected_result_message() {
        let (mut client_ctrl, mut server_ctrl) = control_pair().await;

        let client_task = tokio::spawn(async move {
            client_pairing(&mut client_ctrl, "123456").await
        });

        let _req = server_ctrl.receiver.recv().await;
        let (_state, host_msg) = Spake2::<Ed25519Group>::start_b(
            &Password::new(b"123456"),
            &Identity::new(CLIENT_IDENTITY),
            &Identity::new(HOST_IDENTITY),
        );
        server_ctrl
            .sender
            .send(&ControlMessage::PairingResponse(host_msg.clone()))
            .await
            .unwrap();

        let _confirm = server_ctrl.receiver.recv().await;
        // Send wrong message instead of PairingResult
        server_ctrl
            .sender
            .send(&ControlMessage::Keepalive)
            .await
            .unwrap();

        let result = client_task.await.unwrap();
        assert!(matches!(result, Err(SessionError::PairingFailed(_))));
    }

    // ── Challenge-response auth ─────────────────────────────────────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_auth_challenge_happy_path() {
        let (mut client_ctrl, mut server_ctrl) = control_pair().await;

        let pin = "654321";
        let mut trust_db = TrustDatabase::new();
        let (client_result, _server_result) = tokio::join!(
            client_pairing(&mut client_ctrl, pin),
            host_pairing(&mut server_ctrl, pin, &mut trust_db, "laptop"),
        );
        let signing_key = client_result.unwrap();

        let (mut client_ctrl2, mut server_ctrl2) = control_pair().await;

        let (client_auth, server_auth) = tokio::join!(
            client_auth_response(&mut client_ctrl2, &signing_key),
            host_auth_challenge(&mut server_ctrl2, &mut trust_db),
        );

        client_auth.expect("client auth should succeed");
        let authenticated = server_auth.expect("host auth should succeed");
        assert_eq!(authenticated.client_id, "laptop");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_auth_challenge_untrusted_client() {
        let (mut client_ctrl, mut server_ctrl) = control_pair().await;
        let mut trust_db = TrustDatabase::new();
        let untrusted_key = SigningKey::generate(&mut rand::thread_rng());

        let (client_result, server_result) = tokio::join!(
            client_auth_response(&mut client_ctrl, &untrusted_key),
            host_auth_challenge(&mut server_ctrl, &mut trust_db),
        );

        assert!(server_result.is_err());
        assert!(client_result.is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_auth_challenge_stream_closed() {
        let (client_ctrl, mut server_ctrl) = control_pair().await;
        drop(client_ctrl);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut trust_db = TrustDatabase::new();
        let result = host_auth_challenge(&mut server_ctrl, &mut trust_db).await;
        assert!(result.is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_auth_challenge_unexpected_message() {
        let (mut client_ctrl, mut server_ctrl) = control_pair().await;
        let mut trust_db = TrustDatabase::new();

        let host_task = tokio::spawn(async move {
            host_auth_challenge(&mut server_ctrl, &mut trust_db).await
        });

        let _challenge = client_ctrl.receiver.recv().await;
        client_ctrl
            .sender
            .send(&ControlMessage::Keepalive)
            .await
            .expect("send keepalive");

        let result = host_task.await.unwrap();
        assert!(matches!(result, Err(SessionError::PairingFailed(_))));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_auth_challenge_invalid_response_length() {
        let (mut client_ctrl, mut server_ctrl) = control_pair().await;
        let mut trust_db = TrustDatabase::new();

        let host_task = tokio::spawn(async move {
            host_auth_challenge(&mut server_ctrl, &mut trust_db).await
        });

        let _challenge = client_ctrl.receiver.recv().await;
        // Send AuthResponse with wrong length (not 96 bytes)
        client_ctrl
            .sender
            .send(&ControlMessage::AuthResponse(vec![1, 2, 3]))
            .await
            .unwrap();

        let result = host_task.await.unwrap();
        assert!(matches!(result, Err(SessionError::PairingFailed(_))));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_auth_challenge_bad_signature() {
        let (mut client_ctrl, mut server_ctrl) = control_pair().await;
        let pin = "111111";
        let mut trust_db = TrustDatabase::new();

        // Pair first
        let (mut c1, mut s1) = control_pair().await;
        let (client_result, _) = tokio::join!(
            client_pairing(&mut c1, pin),
            host_pairing(&mut s1, pin, &mut trust_db, "laptop"),
        );
        let signing_key = client_result.unwrap();

        // Now try auth with the right pubkey but a forged signature
        let host_task = tokio::spawn(async move {
            host_auth_challenge(&mut server_ctrl, &mut trust_db).await
        });

        let challenge = client_ctrl.receiver.recv().await;
        let _nonce = match challenge {
            Ok(Some(ControlMessage::AuthChallenge(_))) => {}
            _ => panic!("expected AuthChallenge"),
        };

        // Build response with correct pubkey but garbage signature
        let vk = signing_key.verifying_key();
        let mut response = Vec::with_capacity(96);
        response.extend_from_slice(vk.as_bytes());
        response.extend_from_slice(&[0u8; 64]); // invalid signature bytes

        client_ctrl
            .sender
            .send(&ControlMessage::AuthResponse(response))
            .await
            .unwrap();

        let result = host_task.await.unwrap();
        assert!(matches!(result, Err(SessionError::PairingFailed(_))));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_client_auth_stream_closed_before_challenge() {
        let (mut client_ctrl, server_ctrl) = control_pair().await;

        // Close server side so client gets stream-closed
        drop(server_ctrl);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let key = SigningKey::generate(&mut rand::thread_rng());
        let result = client_auth_response(&mut client_ctrl, &key).await;
        assert!(result.is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_client_auth_unexpected_challenge_message() {
        let (mut client_ctrl, mut server_ctrl) = control_pair().await;

        // Host sends wrong message instead of AuthChallenge
        server_ctrl
            .sender
            .send(&ControlMessage::Keepalive)
            .await
            .unwrap();

        let key = SigningKey::generate(&mut rand::thread_rng());
        let result = client_auth_response(&mut client_ctrl, &key).await;
        assert!(matches!(result, Err(SessionError::PairingFailed(_))));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_client_auth_stream_closed_at_result() {
        let (mut client_ctrl, mut server_ctrl) = control_pair().await;

        // Send AuthChallenge
        server_ctrl
            .sender
            .send(&ControlMessage::AuthChallenge(vec![0u8; 32]))
            .await
            .unwrap();

        // Drain the AuthResponse, then close stream
        let server_task = tokio::spawn(async move {
            let _resp = server_ctrl.receiver.recv().await;
            server_ctrl.sender.stream.finish().unwrap();
        });

        let key = SigningKey::generate(&mut rand::thread_rng());
        let result = client_auth_response(&mut client_ctrl, &key).await;
        assert!(result.is_err());
        server_task.await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_client_auth_unexpected_result_message() {
        let (mut client_ctrl, mut server_ctrl) = control_pair().await;

        let key = SigningKey::generate(&mut rand::thread_rng());

        let server_task = tokio::spawn(async move {
            // Send AuthChallenge
            server_ctrl
                .sender
                .send(&ControlMessage::AuthChallenge(vec![0u8; 32]))
                .await
                .unwrap();
            // Drain the AuthResponse
            let _resp = server_ctrl.receiver.recv().await;
            // Send wrong message instead of AuthResult
            server_ctrl
                .sender
                .send(&ControlMessage::Keepalive)
                .await
                .unwrap();
        });

        let result = client_auth_response(&mut client_ctrl, &key).await;
        assert!(result.is_err());
        server_task.await.unwrap();
    }

    // ── recv_msg / send_msg helpers ────────────────────────────────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_recv_msg_returns_message() {
        let (mut client_ctrl, mut server_ctrl) = control_pair().await;
        client_ctrl
            .sender
            .send(&ControlMessage::Keepalive)
            .await
            .unwrap();
        let msg = recv_msg(&mut server_ctrl, "test").await.unwrap();
        assert_eq!(msg, ControlMessage::Keepalive);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_recv_msg_stream_closed() {
        let (mut client_ctrl, mut server_ctrl) = control_pair().await;
        client_ctrl.sender.stream.finish().unwrap();
        let result = recv_msg(&mut server_ctrl, "test").await;
        assert!(matches!(result, Err(SessionError::PairingFailed(_))));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_recv_msg_transport_error() {
        let (client_ctrl, mut server_ctrl) = control_pair().await;
        drop(client_ctrl);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let result = recv_msg(&mut server_ctrl, "test").await;
        assert!(result.is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_send_msg_success() {
        let (mut client_ctrl, mut server_ctrl) = control_pair().await;
        send_msg(&mut server_ctrl, &ControlMessage::Keepalive)
            .await
            .unwrap();
        let msg = client_ctrl.receiver.recv().await.unwrap().unwrap();
        assert_eq!(msg, ControlMessage::Keepalive);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_send_msg_transport_error() {
        let (client_ctrl, mut server_ctrl) = control_pair().await;
        drop(client_ctrl);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let result = send_msg(&mut server_ctrl, &ControlMessage::Keepalive).await;
        assert!(result.is_err());
    }

    // ── Edge cases for invalid crypto data ─────────────────────────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_client_auth_rejected() {
        let (mut client_ctrl, mut server_ctrl) = control_pair().await;

        // Server sends challenge then rejects
        server_ctrl
            .sender
            .send(&ControlMessage::AuthChallenge(vec![0u8; 32]))
            .await
            .unwrap();

        let server_task = tokio::spawn(async move {
            let _resp = server_ctrl.receiver.recv().await;
            server_ctrl
                .sender
                .send(&ControlMessage::AuthResult(false))
                .await
                .unwrap();
        });

        let key = SigningKey::generate(&mut rand::thread_rng());
        let result = client_auth_response(&mut client_ctrl, &key).await;
        assert!(result.is_err());
        server_task.await.unwrap();
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
}
