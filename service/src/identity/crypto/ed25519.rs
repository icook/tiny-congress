use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

use super::CryptoError;

fn parse_public_key(public_key: &[u8]) -> Result<VerifyingKey, CryptoError> {
    let key_bytes: [u8; 32] = public_key
        .try_into()
        .map_err(|_| CryptoError::InvalidKey("public key must be 32 bytes".to_string()))?;
    VerifyingKey::from_bytes(&key_bytes)
        .map_err(|_| CryptoError::InvalidKey("invalid ed25519 public key".to_string()))
}

fn parse_signature(signature: &[u8]) -> Result<Signature, CryptoError> {
    let sig_bytes: [u8; 64] = signature
        .try_into()
        .map_err(|_| CryptoError::InvalidKey("signature must be 64 bytes".to_string()))?;
    Ok(Signature::from_bytes(&sig_bytes))
}

/// Sign a message using a 32-byte secret key. Intended for tests and utilities.
pub fn sign_message(message: &[u8], secret_key: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let key_bytes: [u8; 32] = secret_key
        .try_into()
        .map_err(|_| CryptoError::InvalidKey("secret key must be 32 bytes".to_string()))?;
    let signing_key = SigningKey::from_bytes(&key_bytes);
    let signature = signing_key.sign(message);
    Ok(signature.to_bytes().to_vec())
}

/// Verify a message with a public key and raw signature bytes.
pub fn verify_signature(
    message: &[u8],
    public_key: &[u8],
    signature: &[u8],
) -> Result<(), CryptoError> {
    let verifying_key = parse_public_key(public_key)?;
    let signature = parse_signature(signature)?;

    verifying_key
        .verify(message, &signature)
        .map_err(|_| CryptoError::VerificationFailed)
}
