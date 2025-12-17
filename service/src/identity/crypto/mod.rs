//! Cryptographic utilities for identity system
//!
//! Re-exports from the shared `tc-crypto` crate for consistency across
//! backend and frontend (via WASM).

// Re-export core functions from tc-crypto
pub use tc_crypto::{decode_base64url_native as decode_base64url, derive_kid, encode_base64url};

// Re-export the error type
pub use tc_crypto::DecodeError;

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
        let decoded = decode_base64url(encoded).expect("decode should succeed");
        assert_eq!(decoded, b"Hello");
    }

    #[test]
    fn test_derive_kid_matches_tc_crypto() {
        // Verify the service produces the same output as tc-crypto directly
        let pubkey = [1u8; 32];
        let kid = derive_kid(&pubkey);
        assert_eq!(kid, "cs1uhCLEB_ttCYaQ8RMLfQ");
    }
}
