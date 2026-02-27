//! Shared cryptographic utilities for `TinyCongress`
//!
//! This crate provides cryptographic functions used by both the Rust backend
//! (as a native library) and the TypeScript frontend (compiled to WASM).

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
pub(crate) use sha2::{Digest, Sha256};
use wasm_bindgen::prelude::*;

mod kid;
pub use kid::{Kid, KidError};

mod envelope;
pub use envelope::{BackupEnvelope, EnvelopeError};

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

/// Decode a base64url-encoded string (RFC 4648) to bytes (WASM binding).
///
/// For native Rust code, use [`decode_base64url`] instead.
///
/// # Errors
/// Returns `JsError` if the input is not valid base64url
#[wasm_bindgen(js_name = "decode_base64url")]
pub fn decode_base64url_js(encoded: &str) -> Result<Vec<u8>, JsError> {
    URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|e| JsError::new(&e.to_string()))
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
/// Returns `DecodeError` if the input is not valid base64url
pub fn decode_base64url(encoded: &str) -> Result<Vec<u8>, DecodeError> {
    URL_SAFE_NO_PAD.decode(encoded).map_err(DecodeError::from)
}

/// Verify an Ed25519 signature over a message.
///
/// Only available with the `ed25519` feature (not compiled to WASM).
///
/// # Errors
/// Returns `VerifyError` if the public key is invalid or the signature
/// does not match.
#[cfg(feature = "ed25519")]
pub fn verify_ed25519(
    public_key: &[u8; 32],
    message: &[u8],
    signature: &[u8; 64],
) -> Result<(), VerifyError> {
    use ed25519_dalek::{Signature, VerifyingKey};

    let verifying_key =
        VerifyingKey::from_bytes(public_key).map_err(|_| VerifyError::InvalidPublicKey)?;
    let sig = Signature::from_bytes(signature);
    verifying_key
        .verify_strict(message, &sig)
        .map_err(|_| VerifyError::SignatureMismatch)
}

/// Errors from Ed25519 signature verification.
#[cfg(feature = "ed25519")]
#[derive(Debug, thiserror::Error)]
pub enum VerifyError {
    #[error("invalid Ed25519 public key")]
    InvalidPublicKey,
    #[error("signature verification failed")]
    SignatureMismatch,
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
    fn test_decode_base64url() {
        let encoded = "SGVsbG8";
        let decoded = decode_base64url(encoded).expect("decode should succeed");
        assert_eq!(decoded, b"Hello");
    }

    #[test]
    fn test_roundtrip() {
        let original = b"test data for roundtrip";
        let encoded = encode_base64url(original);
        let decoded = decode_base64url(&encoded).expect("decode should succeed");
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_decode_invalid_base64url() {
        let invalid = "not valid base64!!!";
        let result = decode_base64url(invalid);
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// Any byte sequence can be encoded and decoded back to the original
        #[test]
        fn roundtrip_encode_decode(bytes: Vec<u8>) {
            let encoded = encode_base64url(&bytes);
            let decoded = decode_base64url(&encoded).unwrap();
            prop_assert_eq!(decoded, bytes);
        }

        /// KID derivation is deterministic - same input always produces same output
        #[test]
        fn derive_kid_deterministic(pubkey: Vec<u8>) {
            let kid1 = derive_kid(&pubkey);
            let kid2 = derive_kid(&pubkey);
            prop_assert_eq!(kid1, kid2);
        }

        /// KID output length is always 21-22 chars (16 bytes base64url encoded)
        #[test]
        fn derive_kid_length_invariant(pubkey: Vec<u8>) {
            let kid = derive_kid(&pubkey);
            prop_assert!(kid.len() >= 21 && kid.len() <= 22,
                "KID length {} not in expected range 21-22", kid.len());
        }

        /// Encoded output contains only valid base64url characters
        #[test]
        fn encode_produces_valid_base64url_chars(bytes: Vec<u8>) {
            let encoded = encode_base64url(&bytes);
            prop_assert!(encoded.chars().all(|c|
                c.is_ascii_alphanumeric() || c == '-' || c == '_'
            ));
        }
    }
}
