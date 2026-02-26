//! Key Identifier (KID) — a validated, type-safe wrapper for key identifiers.
//!
//! A KID is `base64url(SHA-256(pubkey)[0:16])`, always exactly 22 characters
//! of the base64url alphabet `[A-Za-z0-9_-]`.

use crate::{encode_base64url, Digest, Sha256};
use std::fmt;
use std::str::FromStr;

/// A validated key identifier. Guaranteed to be 22 base64url characters.
///
/// Construct via [`Kid::derive`] (from a public key) or [`Kid::from_str`]
/// (from a string, e.g. from a database column).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Kid(String);

/// Error returned when a string is not a valid KID.
#[derive(Debug, thiserror::Error)]
#[error("invalid KID: {reason}")]
pub struct KidError {
    reason: &'static str,
}

/// Expected length of a KID string (16 bytes base64url-encoded without padding).
const KID_LENGTH: usize = 22;

impl Kid {
    /// Derive a KID from a public key.
    ///
    /// Computed as `base64url(SHA-256(pubkey)[0:16])`.
    #[must_use]
    pub fn derive(public_key: &[u8]) -> Self {
        let hash = Sha256::digest(public_key);
        Self(encode_base64url(&hash[..16]))
    }

    /// Return the KID as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Validate that a string is a well-formed KID.
    fn validate(s: &str) -> Result<(), KidError> {
        if s.len() != KID_LENGTH {
            return Err(KidError {
                reason: "must be exactly 22 characters",
            });
        }
        if !s
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(KidError {
                reason: "contains invalid characters (expected base64url)",
            });
        }
        Ok(())
    }
}

impl FromStr for Kid {
    type Err = KidError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::validate(s)?;
        Ok(Self(s.to_string()))
    }
}

impl fmt::Display for Kid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Kid {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl serde::Serialize for Kid {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for Kid {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_produces_valid_kid() {
        let kid = Kid::derive(&[1u8; 32]);
        assert_eq!(kid.as_str().len(), KID_LENGTH);
    }

    #[test]
    fn derive_matches_legacy_derive_kid() {
        let pubkey = [1u8; 32];
        let kid = Kid::derive(&pubkey);
        let legacy = crate::derive_kid(&pubkey);
        assert_eq!(kid.as_str(), &legacy);
    }

    #[test]
    fn from_str_accepts_valid_kid() {
        let kid = Kid::derive(&[0u8; 32]);
        let parsed: Kid = kid.as_str().parse().expect("valid");
        assert_eq!(kid, parsed);
    }

    #[test]
    fn from_str_rejects_wrong_length() {
        assert!("short".parse::<Kid>().is_err());
        assert!("a".repeat(23).parse::<Kid>().is_err());
    }

    #[test]
    fn from_str_rejects_invalid_chars() {
        // 22 chars but contains '!'
        assert!("abcdefghijklmnopqrstu!".parse::<Kid>().is_err());
    }

    #[test]
    fn serde_roundtrip() {
        let kid = Kid::derive(&[42u8; 32]);
        let json = serde_json::to_string(&kid).expect("serialize");
        let parsed: Kid = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(kid, parsed);
    }

    #[test]
    fn display_matches_as_str() {
        let kid = Kid::derive(&[1u8; 32]);
        assert_eq!(format!("{kid}"), kid.as_str());
    }
}
