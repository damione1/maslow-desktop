// API key hashing and constant-time verification for the gRPC/HTTP transport
// auth gate. The plaintext key is never persisted: only its SHA-256 hash
// (hex-encoded) is written to disk, via `ApiSettings::api_key_hash`.

use crate::api_settings::ApiSettings;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

pub fn hash_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    to_hex(&hasher.finalize())
}

/// Constant-time check of `presented` against the stored hash. Always false
/// when no key has ever been generated (`api_key_hash` empty): an empty
/// hash must never authenticate anything.
pub fn check_key(settings: &ApiSettings, presented: &str) -> bool {
    if settings.api_key_hash.is_empty() {
        return false;
    }
    let presented_hash = hash_key(presented);
    let stored = settings.api_key_hash.as_bytes();
    let presented = presented_hash.as_bytes();
    if stored.len() != presented.len() {
        return false;
    }
    stored.ct_eq(presented).into()
}

/// Generate a fresh API key: 32 hex chars (122 bits of entropy from the v4
/// UUID's random bits).
pub fn generate_key() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings_with_hash(hash: &str) -> ApiSettings {
        ApiSettings {
            enabled: true,
            api_key_hash: hash.to_string(),
            port_http: 8642,
            port_grpc: 50051,
        }
    }

    #[test]
    fn empty_hash_never_authenticates() {
        let settings = settings_with_hash("");
        assert!(!check_key(&settings, "anything"));
        assert!(!check_key(&settings, ""));
    }

    #[test]
    fn correct_key_authenticates() {
        let key = generate_key();
        let settings = settings_with_hash(&hash_key(&key));
        assert!(check_key(&settings, &key));
    }

    #[test]
    fn wrong_key_is_rejected() {
        let key = generate_key();
        let settings = settings_with_hash(&hash_key(&key));
        assert!(!check_key(&settings, "wrong-key"));
        assert!(!check_key(&settings, &generate_key()));
    }

    #[test]
    fn key_similar_but_not_identical_is_rejected() {
        let key = generate_key();
        let settings = settings_with_hash(&hash_key(&key));
        // Off-by-one-character check: confirms this isn't accidentally
        // matching on a prefix or a truncated comparison.
        let mut tampered = key.clone();
        tampered.pop();
        tampered.push(if key.ends_with('0') { '1' } else { '0' });
        assert!(!check_key(&settings, &tampered));
    }

    #[test]
    fn hash_key_produces_64_char_hex_string() {
        let hash = hash_key("some-key");
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn hash_key_is_deterministic_and_distinguishes_input() {
        assert_eq!(hash_key("same"), hash_key("same"));
        assert_ne!(hash_key("a"), hash_key("b"));
    }

    #[test]
    fn generate_key_is_32_hex_chars() {
        let key = generate_key();
        assert_eq!(key.len(), 32);
        assert!(key.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn generate_key_produces_unique_keys() {
        assert_ne!(generate_key(), generate_key());
    }
}
