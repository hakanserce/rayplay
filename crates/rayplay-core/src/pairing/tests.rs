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
    let signing = SigningKey::generate(&mut rand_core::OsRng);
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
fn test_decode_invalid_curve_point() {
    // y=2 (little-endian) is not on the Ed25519 curve: the equation
    // x^2 = (y^2 - 1)/(d*y^2 + 1) has no solution mod p.
    let mut bytes = [0u8; 32];
    bytes[0] = 2;
    let encoded = BASE64.encode(bytes);
    let result = decode_public_key(&encoded);
    assert!(
        result.is_err(),
        "expected invalid curve point to be rejected"
    );
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
