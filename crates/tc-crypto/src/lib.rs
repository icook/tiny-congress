//! Shared cryptographic utilities for `TinyCongress`
//!
//! This crate provides cryptographic functions used by both the Rust backend
//! (as a native library) and the TypeScript frontend (compiled to WASM).

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use sha2::{Digest, Sha256};
use wasm_bindgen::prelude::*;

/// Error type for base64url decoding failures
#[derive(Debug, thiserror::Error)]
#[error("invalid base64url encoding: {0}")]
pub struct DecodeError(#[from] base64::DecodeError);

/// Derive a key identifier (KID) from a public key.
///
/// The KID is computed as: `base64url(SHA-256(pubkey)[0:16])`
///
/// This produces a ~22 character string that uniquely identifies a public key
/// while being shorter than the full hash.
///
/// # Arguments
/// * `public_key` - The public key bytes (typically 32 bytes for Ed25519)
///
/// # Returns
/// A base64url-encoded string (without padding) of the first 16 bytes of the SHA-256 hash
#[wasm_bindgen]
#[must_use]
pub fn derive_kid(public_key: &[u8]) -> String {
    let hash = Sha256::digest(public_key);
    // Truncate to first 16 bytes for shorter KIDs
    URL_SAFE_NO_PAD.encode(&hash[..16])
}

/// Encode bytes as base64url (RFC 4648) without padding.
///
/// # Arguments
/// * `bytes` - The bytes to encode
///
/// # Returns
/// A base64url-encoded string without padding characters
#[wasm_bindgen]
#[must_use]
pub fn encode_base64url(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Decode a base64url-encoded string (RFC 4648) to bytes.
///
/// Accepts input with or without padding.
///
/// # Arguments
/// * `encoded` - The base64url-encoded string
///
/// # Returns
/// The decoded bytes
///
/// # Errors
/// Returns `JsError` if the input is not valid base64url
#[wasm_bindgen]
pub fn decode_base64url(encoded: &str) -> Result<Vec<u8>, JsError> {
    URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|e| JsError::new(&e.to_string()))
}

/// Native Rust API for decoding base64url (returns proper Rust error type)
///
/// # Errors
/// Returns `DecodeError` if the input is not valid base64url
pub fn decode_base64url_native(encoded: &str) -> Result<Vec<u8>, DecodeError> {
    URL_SAFE_NO_PAD.decode(encoded).map_err(DecodeError::from)
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
    fn test_derive_kid_known_vector() {
        // Test vector: all-ones pubkey should produce consistent KID
        let pubkey = [1u8; 32];
        let kid = derive_kid(&pubkey);
        // This is the expected output - if this changes, the algorithm changed
        assert_eq!(kid, "cs1uhCLEB_ttCYaQ8RMLfQ");
    }

    #[test]
    fn test_encode_base64url() {
        let bytes = b"Hello";
        let encoded = encode_base64url(bytes);
        assert_eq!(encoded, "SGVsbG8");
    }

    #[test]
    fn test_decode_base64url_native() {
        let encoded = "SGVsbG8";
        let decoded = decode_base64url_native(encoded).expect("decode should succeed");
        assert_eq!(decoded, b"Hello");
    }

    #[test]
    fn test_roundtrip() {
        let original = b"test data for roundtrip";
        let encoded = encode_base64url(original);
        let decoded = decode_base64url_native(&encoded).expect("decode should succeed");
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_decode_invalid_base64url() {
        let invalid = "not valid base64!!!";
        let result = decode_base64url_native(invalid);
        assert!(result.is_err());
    }
}
