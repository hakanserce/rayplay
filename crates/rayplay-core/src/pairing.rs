//! Trust database and pairing utilities for PIN-based client authentication (UC-016).
//!
//! Contains the [`TrustDatabase`] for managing trusted client identities,
//! [`PairingError`] for pairing-specific failures, and helpers for PIN
//! generation and ed25519 public-key encoding.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use chrono::Utc;
use ed25519_dalek::VerifyingKey;
use rand::Rng;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Length of the generated pairing PIN.
const PIN_LENGTH: usize = 6;

/// A trusted client entry stored on the host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedClient {
    /// Human-readable label (e.g. device name).
    pub client_id: String,
    /// Base64-encoded ed25519 public key.
    pub public_key: String,
    /// ISO 8601 timestamp when the client was first paired.
    pub paired_at: String,
    /// ISO 8601 timestamp of the most recent connection.
    pub last_seen: String,
}

/// In-memory trust database backed by a list of [`TrustedClient`] entries.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustDatabase {
    clients: Vec<TrustedClient>,
}

impl TrustDatabase {
    /// Creates an empty trust database.
    #[must_use]
    pub fn new() -> Self {
        Self {
            clients: Vec::new(),
        }
    }

    /// Deserialises a trust database from a JSON string.
    ///
    /// # Errors
    ///
    /// Returns [`PairingError::Serialization`] if the JSON is malformed.
    pub fn from_json(json: &str) -> Result<Self, PairingError> {
        serde_json::from_str(json).map_err(|e| PairingError::Serialization(e.to_string()))
    }

    /// Serialises the trust database to a pretty-printed JSON string.
    ///
    /// # Errors
    ///
    /// Returns [`PairingError::Serialization`] on serialisation failure.
    pub fn to_json(&self) -> Result<String, PairingError> {
        serde_json::to_string_pretty(self).map_err(|e| PairingError::Serialization(e.to_string()))
    }

    /// Adds or replaces a trusted client (matched by `public_key`).
    pub fn add_client(&mut self, client: TrustedClient) {
        self.clients
            .retain(|c| c.public_key != client.public_key);
        self.clients.push(client);
    }

    /// Removes a client by public key. Returns `true` if a client was removed.
    pub fn remove_client(&mut self, public_key: &str) -> bool {
        let before = self.clients.len();
        self.clients.retain(|c| c.public_key != public_key);
        self.clients.len() < before
    }

    /// Returns `true` if the given base64-encoded public key is trusted.
    #[must_use]
    pub fn is_trusted(&self, public_key: &str) -> bool {
        self.clients.iter().any(|c| c.public_key == public_key)
    }

    /// Finds a trusted client by public key.
    #[must_use]
    pub fn find_client(&self, public_key: &str) -> Option<&TrustedClient> {
        self.clients.iter().find(|c| c.public_key == public_key)
    }

    /// Updates the `last_seen` timestamp for a client to the current time.
    pub fn update_last_seen(&mut self, public_key: &str) {
        if let Some(client) = self
            .clients
            .iter_mut()
            .find(|c| c.public_key == public_key)
        {
            client.last_seen = Utc::now().to_rfc3339();
        }
    }

    /// Returns all trusted clients.
    #[must_use]
    pub fn list_clients(&self) -> &[TrustedClient] {
        &self.clients
    }

    /// Returns the number of trusted clients.
    #[must_use]
    pub fn len(&self) -> usize {
        self.clients.len()
    }

    /// Returns `true` if no clients are trusted.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.clients.is_empty()
    }
}

/// Errors specific to the pairing and trust-management layer.
#[derive(Debug, Error)]
pub enum PairingError {
    /// The public key bytes are not a valid ed25519 key.
    #[error("invalid public key: {0}")]
    InvalidPublicKey(String),
    /// JSON serialisation or deserialisation failed.
    #[error("serialization error: {0}")]
    Serialization(String),
    /// The SPAKE2 protocol exchange failed.
    #[error("protocol error: {0}")]
    ProtocolError(String),
    /// The PINs on both sides did not match.
    #[error("PIN mismatch")]
    PinMismatch,
}

/// Generates a random 6-digit zero-padded PIN string.
#[must_use]
pub fn generate_pin() -> String {
    let n: u32 = rand::thread_rng().gen_range(0..1_000_000);
    format!("{n:0>PIN_LENGTH$}")
}

/// Encodes an ed25519 verifying (public) key as a base64 string.
#[must_use]
pub fn encode_public_key(key: &VerifyingKey) -> String {
    BASE64.encode(key.as_bytes())
}

/// Decodes a base64 string into an ed25519 [`VerifyingKey`].
///
/// # Errors
///
/// Returns [`PairingError::InvalidPublicKey`] if the base64 is invalid or
/// the decoded bytes are not a valid ed25519 public key.
pub fn decode_public_key(b64: &str) -> Result<VerifyingKey, PairingError> {
    let bytes = BASE64
        .decode(b64)
        .map_err(|e| PairingError::InvalidPublicKey(e.to_string()))?;
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|v: Vec<u8>| PairingError::InvalidPublicKey(format!("expected 32 bytes, got {}", v.len())))?;
    VerifyingKey::from_bytes(&arr)
        .map_err(|e| PairingError::InvalidPublicKey(e.to_string()))
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;

    use super::*;

    fn sample_client(id: &str, key: &str) -> TrustedClient {
        TrustedClient {
            client_id: id.to_string(),
            public_key: key.to_string(),
            paired_at: "2026-01-01T00:00:00Z".to_string(),
            last_seen: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    fn make_key_pair() -> (SigningKey, VerifyingKey) {
        let signing = SigningKey::generate(&mut rand::thread_rng());
        let verifying = signing.verifying_key();
        (signing, verifying)
    }

    // ── TrustDatabase ───────────────────────────────────────────────────────

    #[test]
    fn test_new_database_is_empty() {
        let db = TrustDatabase::new();
        assert!(db.is_empty());
        assert_eq!(db.len(), 0);
        assert!(db.list_clients().is_empty());
    }

    #[test]
    fn test_default_database_is_empty() {
        let db = TrustDatabase::default();
        assert!(db.is_empty());
    }

    #[test]
    fn test_add_client() {
        let mut db = TrustDatabase::new();
        db.add_client(sample_client("laptop", "key1"));
        assert_eq!(db.len(), 1);
        assert!(!db.is_empty());
    }

    #[test]
    fn test_add_replaces_by_public_key() {
        let mut db = TrustDatabase::new();
        db.add_client(sample_client("laptop-old", "key1"));
        db.add_client(sample_client("laptop-new", "key1"));
        assert_eq!(db.len(), 1);
        assert_eq!(db.find_client("key1").unwrap().client_id, "laptop-new");
    }

    #[test]
    fn test_remove_client_returns_true_when_found() {
        let mut db = TrustDatabase::new();
        db.add_client(sample_client("laptop", "key1"));
        assert!(db.remove_client("key1"));
        assert!(db.is_empty());
    }

    #[test]
    fn test_remove_client_returns_false_when_missing() {
        let mut db = TrustDatabase::new();
        assert!(!db.remove_client("nonexistent"));
    }

    #[test]
    fn test_is_trusted() {
        let mut db = TrustDatabase::new();
        db.add_client(sample_client("laptop", "key1"));
        assert!(db.is_trusted("key1"));
        assert!(!db.is_trusted("key2"));
    }

    #[test]
    fn test_find_client() {
        let mut db = TrustDatabase::new();
        db.add_client(sample_client("laptop", "key1"));
        assert!(db.find_client("key1").is_some());
        assert!(db.find_client("key2").is_none());
    }

    #[test]
    fn test_update_last_seen() {
        let mut db = TrustDatabase::new();
        db.add_client(sample_client("laptop", "key1"));
        let before = db.find_client("key1").unwrap().last_seen.clone();
        db.update_last_seen("key1");
        let after = &db.find_client("key1").unwrap().last_seen;
        assert_ne!(&before, after);
    }

    #[test]
    fn test_update_last_seen_noop_for_missing_key() {
        let mut db = TrustDatabase::new();
        db.update_last_seen("nonexistent");
        assert!(db.is_empty());
    }

    #[test]
    fn test_list_clients_returns_all() {
        let mut db = TrustDatabase::new();
        db.add_client(sample_client("a", "k1"));
        db.add_client(sample_client("b", "k2"));
        assert_eq!(db.list_clients().len(), 2);
    }

    #[test]
    fn test_json_roundtrip() {
        let mut db = TrustDatabase::new();
        db.add_client(sample_client("laptop", "key1"));
        db.add_client(sample_client("phone", "key2"));
        let json = db.to_json().unwrap();
        let restored = TrustDatabase::from_json(&json).unwrap();
        assert_eq!(db, restored);
    }

    #[test]
    fn test_json_roundtrip_empty() {
        let db = TrustDatabase::new();
        let json = db.to_json().unwrap();
        let restored = TrustDatabase::from_json(&json).unwrap();
        assert_eq!(db, restored);
    }

    #[test]
    fn test_from_json_malformed() {
        let result = TrustDatabase::from_json("not json");
        assert!(result.is_err());
    }

    #[test]
    fn test_trusted_client_clone_equals_original() {
        let c = sample_client("laptop", "key1");
        assert_eq!(c.clone(), c);
    }

    #[test]
    fn test_trusted_client_debug_format() {
        let c = sample_client("laptop", "key1");
        let dbg = format!("{c:?}");
        assert!(dbg.contains("laptop"));
        assert!(dbg.contains("key1"));
    }

    #[test]
    fn test_database_clone_equals_original() {
        let mut db = TrustDatabase::new();
        db.add_client(sample_client("laptop", "key1"));
        assert_eq!(db.clone(), db);
    }

    #[test]
    fn test_database_debug_format() {
        let db = TrustDatabase::new();
        assert!(format!("{db:?}").contains("TrustDatabase"));
    }

    // ── generate_pin ────────────────────────────────────────────────────────

    #[test]
    fn test_generate_pin_length() {
        let pin = generate_pin();
        assert_eq!(pin.len(), PIN_LENGTH);
    }

    #[test]
    fn test_generate_pin_is_numeric() {
        let pin = generate_pin();
        assert!(pin.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn test_generate_pin_varies() {
        let pins: Vec<_> = (0..10).map(|_| generate_pin()).collect();
        let unique: std::collections::HashSet<_> = pins.iter().collect();
        assert!(unique.len() > 1);
    }

    // ── encode / decode public key ──────────────────────────────────────────

    #[test]
    fn test_public_key_roundtrip() {
        let (_, vk) = make_key_pair();
        let encoded = encode_public_key(&vk);
        let decoded = decode_public_key(&encoded).unwrap();
        assert_eq!(vk, decoded);
    }

    #[test]
    fn test_decode_invalid_base64() {
        let result = decode_public_key("not!valid!base64!!!");
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_wrong_length() {
        let encoded = BASE64.encode(b"too short");
        let result = decode_public_key(&encoded);
        assert!(result.is_err());
    }

    #[test]
    fn test_encode_different_keys_differ() {
        let (_, vk1) = make_key_pair();
        let (_, vk2) = make_key_pair();
        assert_ne!(encode_public_key(&vk1), encode_public_key(&vk2));
    }

    // ── PairingError ────────────────────────────────────────────────────────

    #[test]
    fn test_pairing_error_invalid_public_key_display() {
        let e = PairingError::InvalidPublicKey("bad bytes".into());
        assert_eq!(e.to_string(), "invalid public key: bad bytes");
    }

    #[test]
    fn test_pairing_error_serialization_display() {
        let e = PairingError::Serialization("bad json".into());
        assert_eq!(e.to_string(), "serialization error: bad json");
    }

    #[test]
    fn test_pairing_error_protocol_error_display() {
        let e = PairingError::ProtocolError("timeout".into());
        assert_eq!(e.to_string(), "protocol error: timeout");
    }

    #[test]
    fn test_pairing_error_pin_mismatch_display() {
        let e = PairingError::PinMismatch;
        assert_eq!(e.to_string(), "PIN mismatch");
    }

    #[test]
    fn test_pairing_error_debug_format() {
        let e = PairingError::PinMismatch;
        assert!(format!("{e:?}").contains("PinMismatch"));
    }
}
