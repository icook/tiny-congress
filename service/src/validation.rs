//! Request validation utilities using the validator crate.
//!
//! This module provides custom validators and helpers for validating
//! HTTP request payloads.
//!
//! # Usage
//!
//! ```ignore
//! use validator::Validate;
//! use crate::validation::validate_base64url_ed25519_pubkey;
//!
//! #[derive(Validate)]
//! struct SignupRequest {
//!     #[validate(length(min = 1, max = 64))]
//!     username: String,
//!
//!     #[validate(custom(function = "validate_base64url_ed25519_pubkey"))]
//!     root_pubkey: String,
//! }
//! ```

use validator::ValidationError;

/// Validates that a string is a valid base64url-encoded Ed25519 public key (32 bytes).
///
/// # Errors
///
/// Returns a `ValidationError` if:
/// - The string is not valid base64url encoding
/// - The decoded bytes are not exactly 32 bytes (Ed25519 key size)
pub fn validate_base64url_ed25519_pubkey(value: &str) -> Result<(), ValidationError> {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

    let bytes = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| ValidationError::new("invalid_base64url"))?;

    if bytes.len() != 32 {
        return Err(ValidationError::new("invalid_pubkey_length"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_pubkey() {
        // 32 bytes encoded as base64url (no padding)
        let valid = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        assert!(validate_base64url_ed25519_pubkey(valid).is_ok());
    }

    #[test]
    fn test_invalid_base64() {
        let invalid = "not-valid-base64!!!";
        let err = validate_base64url_ed25519_pubkey(invalid).unwrap_err();
        assert_eq!(err.code.as_ref(), "invalid_base64url");
    }

    #[test]
    fn test_wrong_length() {
        // 16 bytes instead of 32
        let short = "AAAAAAAAAAAAAAAAAAAAAA";
        let err = validate_base64url_ed25519_pubkey(short).unwrap_err();
        assert_eq!(err.code.as_ref(), "invalid_pubkey_length");
    }
}
