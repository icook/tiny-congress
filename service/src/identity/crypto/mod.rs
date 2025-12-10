mod canonical;
mod ed25519;
mod envelope;
mod kid;

pub use canonical::canonicalize_value;
pub use ed25519::{sign_message, verify_signature};
pub use envelope::{verify_envelope, EnvelopeSigner, SignedEnvelope};
pub use kid::derive_kid;

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("canonicalization failed: {0}")]
    Canonicalization(#[from] serde_json::Error),
    #[error("invalid key: {0}")]
    InvalidKey(String),
    #[error("invalid format: {0}")]
    InvalidFormat(String),
    #[error("kid mismatch")]
    KidMismatch,
    #[error("signature verification failed")]
    VerificationFailed,
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    use serde_json::json;

    const TEST_SECRET_KEY: [u8; 32] = [
        0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
        25, 26, 27, 28, 29, 30, 31,
    ];

    fn test_keypair() -> (Vec<u8>, Vec<u8>) {
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&TEST_SECRET_KEY);
        let verifying_key = signing_key.verifying_key();
        (
            signing_key.to_bytes().to_vec(),
            verifying_key.to_bytes().to_vec(),
        )
    }

    #[test]
    fn canonicalization_is_stable() {
        let value = json!({"b":2,"a":1});
        let first = canonicalize_value(&value).unwrap();
        let second = canonicalize_value(&value).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn derive_kid_matches_expected() {
        let (_, public_key) = test_keypair();
        let kid = derive_kid(&public_key);
        let kid_again = derive_kid(&public_key);
        assert_eq!(kid, kid_again);
    }

    #[test]
    fn sign_and_verify_round_trip() {
        let (secret_key, public_key) = test_keypair();
        let message = b"hello world";
        let sig = sign_message(message, &secret_key).unwrap();
        verify_signature(message, &public_key, &sig).unwrap();
    }

    #[test]
    fn verify_envelope_checks_kid_and_signature() {
        let (secret_key, public_key) = test_keypair();
        let kid = derive_kid(&public_key);

        let envelope = SignedEnvelope {
            v: 1,
            payload_type: "Test".to_string(),
            payload: json!({"prev_hash": null, "body": "ok"}),
            signer: EnvelopeSigner {
                account_id: None,
                device_id: None,
                kid: kid.clone(),
            },
            sig: String::new(), // filled below
        };

        let signing_bytes = envelope.canonical_signing_bytes().unwrap();
        let signature = sign_message(&signing_bytes, &secret_key).unwrap();
        let mut envelope = envelope;
        envelope.sig = URL_SAFE_NO_PAD.encode(signature);

        verify_envelope(&envelope, &public_key).unwrap();

        // Tamper kid to trigger mismatch error
        let mut bad_envelope = envelope.clone();
        bad_envelope.signer.kid = "different".to_string();
        let result = verify_envelope(&bad_envelope, &public_key);
        assert!(matches!(result, Err(CryptoError::KidMismatch)));
    }

    #[test]
    fn envelope_prev_hash_parses() {
        let (_, public_key) = test_keypair();
        let kid = derive_kid(&public_key);
        let envelope = SignedEnvelope {
            v: 1,
            payload_type: "Test".to_string(),
            payload: json!({"prev_hash": "c29tZS1kYXRh"}),
            signer: EnvelopeSigner {
                account_id: None,
                device_id: None,
                kid,
            },
            sig: "".to_string(),
        };

        let parsed = envelope.prev_hash_bytes().unwrap();
        assert!(parsed.is_some());
    }
}
