use serde_json::Value;

use super::CryptoError;

/// Canonicalize JSON using RFC 8785 (JCS).
pub fn canonicalize_value(value: &Value) -> Result<Vec<u8>, CryptoError> {
    serde_jcs::to_vec(value).map_err(CryptoError::Canonicalization)
}
