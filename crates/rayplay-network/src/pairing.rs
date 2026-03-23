//! PIN-based pairing and trusted-client authentication over the control channel (UC-016).
//!
//! Implements the SPAKE2 pairing flow from [ADR-007](../../docs/adr/ADR-007.md)
//! and a challenge-response protocol for previously-paired clients.

use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use hmac::Mac;
use rayplay_core::pairing::{TrustDatabase, TrustedClient, encode_public_key};
use rayplay_core::session::{ClientIntent, ControlMessage, PairingOutcome, SessionError};
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
    let client_msg = match control.recv_msg("pairing").await? {
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
    control
        .send_msg(&ControlMessage::PairingResponse(host_msg))
        .await?;

    // 4. Complete SPAKE2 and derive shared key
    let host_key = state
        .finish(&client_msg)
        .map_err(|e| SessionError::PairingFailed(format!("SPAKE2 finish failed: {e}")))?;

    // 5. Wait for PairingConfirm(pubkey + hmac)
    let confirm_payload = match control.recv_msg("pairing").await? {
        ControlMessage::PairingConfirm(payload) => payload,
        other => {
            return Err(SessionError::PairingFailed(format!(
                "expected PairingConfirm, got {other:?}"
            )));
        }
    };

    // 6. Validate confirm payload: pubkey(32) + hmac(32)
    if confirm_payload.len() != 64 {
        let outcome = PairingOutcome::Rejected("Invalid payload length".to_string());
        control
            .send_msg(&ControlMessage::PairingResult(outcome))
            .await?;
        return Err(SessionError::PairingFailed(
            "invalid confirm payload length".to_string(),
        ));
    }
    let (pubkey_bytes, received_hmac) = confirm_payload.split_at(32);

    // Convert host_key to fixed-size array for HMAC functions
    let host_key_array: [u8; 32] = host_key
        .try_into()
        .map_err(|_| SessionError::PairingFailed("host key must be 32 bytes".to_string()))?;

    if compute_hmac(&host_key_array, pubkey_bytes)
        .verify_slice(received_hmac)
        .is_err()
    {
        let outcome = PairingOutcome::Rejected("PIN mismatch".to_string());
        control
            .send_msg(&ControlMessage::PairingResult(outcome))
            .await?;
        return Err(SessionError::PairingFailed("PIN mismatch".to_string()));
    }

    // 7. Parse and store the client's public key
    let public_key_array: [u8; 32] = pubkey_bytes
        .try_into()
        .map_err(|_| SessionError::PairingFailed("public key must be 32 bytes".to_string()))?;
    let verifying_key = VerifyingKey::from_bytes(&public_key_array)
        .map_err(|e| SessionError::PairingFailed(format!("invalid public key: {e}")))?;
    let encoded_key = encode_public_key(&verifying_key);

    let client_label = if client_id == "unknown-client" {
        default_client_label()
    } else {
        client_id.to_string()
    };
    let now = chrono::Utc::now().to_rfc3339();
    let trusted_client = TrustedClient {
        client_id: client_label,
        public_key: encoded_key,
        paired_at: now.clone(),
        last_seen: now,
    };
    trust_db.add_client(trusted_client.clone());

    // 8. Send successful result
    let outcome = PairingOutcome::Accepted;
    control
        .send_msg(&ControlMessage::PairingResult(outcome))
        .await?;

    Ok(trusted_client)
}

/// Runs the client side of the SPAKE2 pairing exchange.
///
/// Generates an ed25519 key pair, executes the SPAKE2 protocol with the given
/// `pin`, and returns the signing key on success.
///
/// # Errors
///
/// Returns [`SessionError::PairingFailed`] on PIN mismatch, protocol errors,
/// or if the host rejects the pairing.
pub async fn client_pairing(
    control: &mut ControlChannel,
    pin: &str,
) -> Result<SigningKey, SessionError> {
    // 1. Send ClientHello to declare pairing intent
    control
        .send_msg(&ControlMessage::ClientHello(ClientIntent::Pair))
        .await?;

    // 2. Generate ed25519 key pair
    let signing_key = SigningKey::generate(&mut rand_core::OsRng);
    let verifying_key = signing_key.verifying_key();

    // 3. Start SPAKE2 as side A
    let (state, client_msg) = Spake2::<Ed25519Group>::start_a(
        &Password::new(pin.as_bytes()),
        &Identity::new(CLIENT_IDENTITY),
        &Identity::new(HOST_IDENTITY),
    );

    // 4. Send PairingRequest(client_spake2_msg)
    control
        .send_msg(&ControlMessage::PairingRequest(client_msg))
        .await?;

    // 5. Wait for PairingResponse(host_spake2_msg)
    let host_msg = match control.recv_msg("pairing").await? {
        ControlMessage::PairingResponse(msg) => msg,
        other => {
            return Err(SessionError::PairingFailed(format!(
                "expected PairingResponse, got {other:?}"
            )));
        }
    };

    // 6. Complete SPAKE2 and derive shared key
    let client_key = state
        .finish(&host_msg)
        .map_err(|e| SessionError::PairingFailed(format!("SPAKE2 finish failed: {e}")))?;

    // 7. Build confirm payload: pubkey(32) + hmac(32)
    let pubkey_bytes = verifying_key.as_bytes();

    // Convert client_key to fixed-size array for HMAC functions
    let client_key_array: [u8; 32] = client_key
        .try_into()
        .map_err(|_| SessionError::PairingFailed("client key must be 32 bytes".to_string()))?;

    let hmac = hmac_bytes(&client_key_array, pubkey_bytes);
    let mut confirm_payload = Vec::with_capacity(64);
    confirm_payload.extend_from_slice(pubkey_bytes);
    confirm_payload.extend_from_slice(&hmac);

    control
        .send_msg(&ControlMessage::PairingConfirm(confirm_payload))
        .await?;

    // 8. Wait for PairingResult
    let result = match control.recv_msg("pairing").await? {
        ControlMessage::PairingResult(outcome) => outcome,
        other => {
            return Err(SessionError::PairingFailed(format!(
                "expected PairingResult, got {other:?}"
            )));
        }
    };

    match result {
        PairingOutcome::Accepted => Ok(signing_key),
        PairingOutcome::Rejected(reason) => Err(SessionError::PairingFailed(format!(
            "pairing rejected: {reason}"
        ))),
    }
}

/// Runs the host side of challenge-response authentication for trusted clients.
///
/// Sends a random nonce challenge, waits for the client's signed response,
/// and verifies the signature against the trust database.
///
/// # Errors
///
/// Returns [`SessionError::PairingFailed`] if the client is not trusted or
/// signature verification fails.
pub async fn host_auth_challenge(
    control: &mut ControlChannel,
    trust_db: &mut TrustDatabase,
) -> Result<TrustedClient, SessionError> {
    // 1. Generate and send random nonce
    let mut nonce = [0u8; NONCE_LEN];
    rand::Rng::fill(&mut rand::rng(), &mut nonce);
    control
        .send_msg(&ControlMessage::AuthChallenge(nonce.to_vec()))
        .await?;

    // 2. Wait for AuthResponse(pubkey + signature)
    let response_payload = match control.recv_msg("auth").await? {
        ControlMessage::AuthResponse(payload) => payload,
        other => {
            return Err(SessionError::PairingFailed(format!(
                "expected AuthResponse, got {other:?}"
            )));
        }
    };

    // 3. Parse response: pubkey(32) + signature(64)
    if response_payload.len() != 96 {
        control.send_msg(&ControlMessage::AuthResult(false)).await?;
        return Err(SessionError::PairingFailed(format!(
            "invalid response length: {} (expected 96)",
            response_payload.len()
        )));
    }
    let (pubkey_bytes, signature_bytes) = response_payload.split_at(32);

    // 4. Check if the public key is trusted
    let pubkey_array: [u8; 32] = pubkey_bytes
        .try_into()
        .map_err(|_| SessionError::PairingFailed("public key must be 32 bytes".to_string()))?;
    let verifying_key = VerifyingKey::from_bytes(&pubkey_array)
        .map_err(|e| SessionError::PairingFailed(format!("invalid public key: {e}")))?;
    let encoded_key = encode_public_key(&verifying_key);

    if !trust_db.is_trusted(&encoded_key) {
        control.send_msg(&ControlMessage::AuthResult(false)).await?;
        return Err(SessionError::PairingFailed(
            "client not trusted".to_string(),
        ));
    }

    // 5. Verify the signature
    let signature = ed25519_dalek::Signature::from_slice(signature_bytes)
        .map_err(|e| SessionError::PairingFailed(format!("invalid signature: {e}")))?;
    if verifying_key.verify(&nonce, &signature).is_err() {
        control.send_msg(&ControlMessage::AuthResult(false)).await?;
        return Err(SessionError::PairingFailed(
            "signature verification failed".to_string(),
        ));
    }

    // 6. Update last_seen and send success
    trust_db.update_last_seen(&encoded_key);
    let client = trust_db
        .find_client(&encoded_key)
        .ok_or_else(|| {
            SessionError::PairingFailed("client disappeared from trust database".to_string())
        })?
        .clone();
    control.send_msg(&ControlMessage::AuthResult(true)).await?;

    Ok(client)
}

/// Runs the client side of the challenge-response authentication.
///
/// Waits for a challenge nonce, signs it with the given key, and sends the
/// response. The host verifies that the public key is trusted.
///
/// # Arguments
///
/// * `control` — The QUIC control channel
/// * `signing_key` — The client's ed25519 signing key
///
/// # Errors
///
/// Returns [`SessionError::PairingFailed`] if the host rejects authentication.
pub async fn client_auth_response(
    control: &mut ControlChannel,
    signing_key: &SigningKey,
) -> Result<(), SessionError> {
    // 1. Send ClientHello to declare auth intent
    control
        .send_msg(&ControlMessage::ClientHello(ClientIntent::Auth))
        .await?;

    // 2. Wait for AuthChallenge(nonce)
    let nonce = match control.recv_msg("auth").await? {
        ControlMessage::AuthChallenge(nonce) => nonce,
        other => {
            return Err(SessionError::PairingFailed(format!(
                "expected AuthChallenge, got {other:?}"
            )));
        }
    };

    // 3. Sign the nonce with our key
    let signature = signing_key.sign(&nonce);
    let verifying_key = signing_key.verifying_key();

    // 4. Send AuthResponse(pubkey + signature)
    let mut response_payload = Vec::with_capacity(96);
    response_payload.extend_from_slice(verifying_key.as_bytes());
    response_payload.extend_from_slice(&signature.to_bytes());
    control
        .send_msg(&ControlMessage::AuthResponse(response_payload))
        .await?;

    // 5. Wait for AuthResult
    let success = match control.recv_msg("auth").await? {
        ControlMessage::AuthResult(success) => success,
        other => {
            return Err(SessionError::PairingFailed(format!(
                "expected AuthResult, got {other:?}"
            )));
        }
    };

    if success {
        Ok(())
    } else {
        Err(SessionError::PairingFailed(
            "authentication rejected".to_string(),
        ))
    }
}

// ── Helper functions ─────────────────────────────────────────────────────────

/// Computes HMAC-SHA256 and returns the Mac instance for constant-time verification.
fn compute_hmac(key: &[u8; 32], data: &[u8]) -> hmac::Hmac<sha2::Sha256> {
    use hmac::Hmac;
    use sha2::Sha256;

    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC-SHA256 accepts any key length");
    mac.update(data);
    mac
}

/// Computes HMAC-SHA256 and returns the raw bytes (for building payloads).
fn hmac_bytes(key: &[u8; 32], data: &[u8]) -> [u8; 32] {
    compute_hmac(key, data).finalize().into_bytes().into()
}

/// Generates a human-readable client label using the username.
fn default_client_label() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown-client".to_string())
}

#[cfg(test)]
mod tests;
