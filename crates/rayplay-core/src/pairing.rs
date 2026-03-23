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
        self.clients.retain(|c| c.public_key != client.public_key);
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
        if let Some(client) = self.clients.iter_mut().find(|c| c.public_key == public_key) {
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
}

/// Generates a random 6-digit zero-padded PIN string.
#[must_use]
pub fn generate_pin() -> String {
    let n: u32 = rand::rng().random_range(0..1_000_000);
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
    let arr: [u8; 32] = bytes.try_into().map_err(|v: Vec<u8>| {
        PairingError::InvalidPublicKey(format!("expected 32 bytes, got {}", v.len()))
    })?;
    VerifyingKey::from_bytes(&arr).map_err(|e| PairingError::InvalidPublicKey(e.to_string()))
}

#[cfg(test)]
mod tests;
