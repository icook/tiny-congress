use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use sha2::{Digest, Sha256};

/// Derive a key identifier (kid) from a public key.
///
/// # Must Use
/// Always consume the returned string; dropping it will skip caller validation.
#[must_use]
pub fn derive_kid(public_key: &[u8]) -> String {
    let hash = Sha256::digest(public_key);
    URL_SAFE_NO_PAD.encode(hash)
}
