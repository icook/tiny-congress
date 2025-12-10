mod canonical;
mod ed25519;
mod envelope;
mod kid;

pub use canonical::canonicalize_value;
pub use ed25519::{sign_message, verify_signature};
pub use envelope::{encode_base64url, verify_envelope, EnvelopeSigner, SignedEnvelope};
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
    use anyhow::Result;
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
    fn canonicalization_is_stable() -> Result<()> {
        let value = json!({"b":2,"a":1});
        let first = canonicalize_value(&value)?;
        let second = canonicalize_value(&value)?;
        assert_eq!(first, second);
        Ok(())
    }

    #[test]
    fn derive_kid_matches_expected() {
        let (_, public_key) = test_keypair();
        let kid = derive_kid(&public_key);
        let kid_again = derive_kid(&public_key);
        assert_eq!(kid, kid_again);
    }

    #[test]
    fn sign_and_verify_round_trip() -> Result<()> {
        let (secret_key, public_key) = test_keypair();
        let message = b"hello world";
        let sig = sign_message(message, &secret_key)?;
        verify_signature(message, &public_key, &sig)?;
        Ok(())
    }

    #[test]
    fn sign_and_verify_matches_expected_signature() -> Result<()> {
        let (secret_key, public_key) = test_keypair();
        let envelope = SignedEnvelope {
            v: 1,
            payload_type: "Test".to_string(),
            payload: json!({"body": {"foo": "bar"}, "prev_hash": null}),
            signer: EnvelopeSigner {
                account_id: None,
                device_id: None,
                kid: derive_kid(&public_key),
            },
            sig: String::new(),
        };

        let signing_bytes = envelope.canonical_signing_bytes()?;
        let signature = sign_message(&signing_bytes, &secret_key)?;
        let encoded_sig = URL_SAFE_NO_PAD.encode(&signature);

        // Assert the encoded signature is deterministic and verifies.
        assert_eq!(
            encoded_sig,
            "hYIISBD5RFoDlp969r48FHviKhIjSfpR3K2aKKb3OAq7hffkI042G1mCvU3MD7AsGpFuzSeZOojtpIBU5gigCw"
        );
        verify_signature(&signing_bytes, &public_key, &signature)?;
        Ok(())
    }

    #[test]
    fn verify_envelope_checks_kid_and_signature() -> Result<()> {
        let (secret_key, public_key) = test_keypair();
        let kid = derive_kid(&public_key);

        let envelope = SignedEnvelope {
            v: 1,
            payload_type: "Test".to_string(),
            payload: json!({"prev_hash": null, "body": "ok"}),
            signer: EnvelopeSigner {
                account_id: None,
                device_id: None,
                kid,
            },
            sig: String::new(), // filled below
        };

        let signing_bytes = envelope.canonical_signing_bytes()?;
        let signature = sign_message(&signing_bytes, &secret_key)?;
        let mut envelope = envelope;
        envelope.sig = URL_SAFE_NO_PAD.encode(signature);

        verify_envelope(&envelope, &public_key)?;

        // Tamper kid; signature now fails because signer metadata changed
        let mut bad_envelope = envelope;
        bad_envelope.signer.kid = "different".to_string();
        let result = verify_envelope(&bad_envelope, &public_key);
        assert!(matches!(result, Err(CryptoError::VerificationFailed)));
        Ok(())
    }

    #[test]
    fn envelope_prev_hash_parses() -> Result<()> {
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
            sig: String::new(),
        };

        let parsed = envelope.prev_hash_bytes()?;
        assert!(parsed.is_some());
        Ok(())
    }

    #[test]
    fn invalid_base64_signature_fails() {
        let (_, public_key) = test_keypair();
        let kid = derive_kid(&public_key);
        let envelope = SignedEnvelope {
            v: 1,
            payload_type: "Test".to_string(),
            payload: json!({"body": {}}),
            signer: EnvelopeSigner {
                account_id: None,
                device_id: None,
                kid,
            },
            sig: "!!!not-base64!!!".to_string(),
        };

        let err = verify_envelope(&envelope, &public_key);
        assert!(matches!(err, Err(CryptoError::InvalidFormat(_))));
    }

    #[test]
    fn tampered_payload_rejected() -> Result<()> {
        let (secret_key, public_key) = test_keypair();
        let kid = derive_kid(&public_key);
        let mut envelope = SignedEnvelope {
            v: 1,
            payload_type: "Test".to_string(),
            payload: json!({"body": {"foo": "bar"}, "prev_hash": null}),
            signer: EnvelopeSigner {
                account_id: None,
                device_id: None,
                kid,
            },
            sig: String::new(),
        };

        let signing_bytes = envelope.canonical_signing_bytes()?;
        let signature = sign_message(&signing_bytes, &secret_key)?;
        envelope.sig = URL_SAFE_NO_PAD.encode(signature);

        // Tamper payload after signing
        envelope.payload = json!({"body": {"foo": "baz"}, "prev_hash": null});
        let err = verify_envelope(&envelope, &public_key);
        assert!(matches!(err, Err(CryptoError::VerificationFailed)));
        Ok(())
    }
}
