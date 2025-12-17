//! Cryptographic utilities for identity system

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use sha2::{Digest, Sha256};

/// Derive a key identifier (kid) from a public key.
/// KID = base64url(SHA-256(pubkey)[0:16])
#[must_use]
pub fn derive_kid(public_key: &[u8]) -> String {
    let hash = Sha256::digest(public_key);
    // Truncate to first 16 bytes for shorter KIDs
    URL_SAFE_NO_PAD.encode(&hash[..16])
}

/// Decode a base64url-encoded string to bytes
///
/// # Errors
/// Returns `DecodeError` if the input is not valid base64url
pub fn decode_base64url(encoded: &str) -> Result<Vec<u8>, base64::DecodeError> {
    URL_SAFE_NO_PAD.decode(encoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_kid_deterministic() {
        let pubkey = [1u8; 32];
        let kid1 = derive_kid(&pubkey);
        let kid2 = derive_kid(&pubkey);
        assert_eq!(kid1, kid2);
    }

    #[test]
    fn test_derive_kid_length() {
        let pubkey = [0u8; 32];
        let kid = derive_kid(&pubkey);
        // 16 bytes -> ~22 base64 chars (without padding)
        assert!(kid.len() >= 21 && kid.len() <= 22);
    }

    #[test]
    fn test_decode_base64url() {
        let encoded = "SGVsbG8";
        let decoded = decode_base64url(encoded).unwrap();
        assert_eq!(decoded, b"Hello");
    }
}
