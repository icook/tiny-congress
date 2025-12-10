use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use super::{canonicalize_value, derive_kid, verify_signature, CryptoError};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnvelopeSigner {
    pub account_id: Option<Uuid>,
    pub device_id: Option<Uuid>,
    pub kid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignedEnvelope {
    pub v: u8,
    pub payload_type: String,
    pub payload: Value,
    pub signer: EnvelopeSigner,
    pub sig: String,
}

impl SignedEnvelope {
    /// Canonical bytes used for signing: `payload_type` + payload + signer.
    ///
    /// # Errors
    /// Returns an error when canonicalization fails.
    pub fn canonical_signing_bytes(&self) -> Result<Vec<u8>, CryptoError> {
        let canonical_target = json!({
            "payload_type": self.payload_type,
            "payload": self.payload,
            "signer": self.signer,
        });

        canonicalize_value(&canonical_target)
    }

    /// Decode the signature from base64url.
    ///
    /// # Errors
    /// Returns an error when the signature is not valid base64url.
    pub fn signature_bytes(&self) -> Result<Vec<u8>, CryptoError> {
        decode_base64url(&self.sig)
    }

    /// Optional `prev_hash` extracted from payload (base64url).
    ///
    /// # Errors
    /// Returns an error when `prev_hash` is present but not a base64url string.
    pub fn prev_hash_bytes(&self) -> Result<Option<Vec<u8>>, CryptoError> {
        match self.payload.get("prev_hash") {
            Some(Value::String(encoded)) => Ok(Some(decode_base64url(encoded)?)),
            Some(Value::Null) | None => Ok(None),
            _ => Err(CryptoError::InvalidFormat(
                "prev_hash must be a base64url string when present".to_string(),
            )),
        }
    }
}

/// Verify an envelope against a public key and embedded kid.
///
/// # Errors
/// Returns an error when canonicalization, decoding, kid verification, or signature verification fails.
pub fn verify_envelope(envelope: &SignedEnvelope, signer_pubkey: &[u8]) -> Result<(), CryptoError> {
    let canonical_bytes = envelope.canonical_signing_bytes()?;
    let sig_bytes = envelope.signature_bytes()?;

    verify_signature(&canonical_bytes, signer_pubkey, &sig_bytes)?;

    let expected_kid = derive_kid(signer_pubkey);
    if envelope.signer.kid != expected_kid {
        return Err(CryptoError::KidMismatch);
    }

    Ok(())
}

#[inline]
#[must_use]
pub fn encode_base64url(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

fn decode_base64url(data: &str) -> Result<Vec<u8>, CryptoError> {
    URL_SAFE_NO_PAD
        .decode(data)
        .map_err(|_| CryptoError::InvalidFormat("invalid base64url".to_string()))
}
