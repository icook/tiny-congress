//! Backup envelope — binary format for encrypted root key backups.
//!
//! Argon2id-only. Fixed layout:
//!
//! | Offset | Size | Field                |
//! |--------|------|----------------------|
//! | 0      | 1    | version (0x01)       |
//! | 1      | 1    | kdf_id (0x01)        |
//! | 2      | 4    | m_cost (LE u32)      |
//! | 6      | 4    | t_cost (LE u32)      |
//! | 10     | 4    | p_cost (LE u32)      |
//! | 14     | 16   | salt                 |
//! | 30     | 12   | nonce                |
//! | 42     | N    | ciphertext (min 48)  |

use std::fmt;

/// Current envelope version.
const VERSION: u8 = 0x01;
/// KDF identifier for Argon2id.
const KDF_ARGON2ID: u8 = 0x01;
/// Fixed header size: version(1) + kdf(1) + m(4) + t(4) + p(4) + salt(16) + nonce(12) = 42
const HEADER_SIZE: usize = 42;
/// Minimum ciphertext: 32 (key) + 16 (GCM tag) = 48
const MIN_CIPHERTEXT: usize = 48;
/// Minimum total envelope size.
const MIN_ENVELOPE_SIZE: usize = HEADER_SIZE + MIN_CIPHERTEXT; // 90
/// Maximum accepted envelope size (defence-in-depth).
const MAX_ENVELOPE_SIZE: usize = 4096;
/// Offset where the 16-byte salt begins.
const SALT_OFFSET: usize = 14;

/// Minimum acceptable Argon2id memory cost (64 MiB).
/// Matches OWASP 2024 recommendation for Argon2id.
const MIN_M_COST: u32 = 65536;
/// Minimum acceptable Argon2id time cost (iterations).
const MIN_T_COST: u32 = 3;
/// Minimum acceptable Argon2id parallelism.
const MIN_P_COST: u32 = 1;

/// A parsed and validated encrypted backup envelope.
///
/// The envelope is always Argon2id version 1. Construct via [`BackupEnvelope::parse`]
/// (from raw bytes, e.g. from a client request) or [`BackupEnvelope::build`]
/// (from individual fields, e.g. in tests).
pub struct BackupEnvelope {
    salt: [u8; 16],
    version: u8,
    raw: Vec<u8>,
}

/// Errors from envelope parsing or construction.
#[derive(Debug, thiserror::Error)]
pub enum EnvelopeError {
    #[error("Encrypted backup envelope too small")]
    TooSmall,
    #[error("Encrypted backup envelope too large")]
    TooLarge,
    #[error("Unsupported backup envelope version")]
    UnsupportedVersion,
    #[error("Unsupported KDF (only Argon2id is accepted)")]
    UnsupportedKdf,
    #[error("Ciphertext too small (minimum 48 bytes)")]
    CiphertextTooSmall,
    #[error("KDF parameters too weak (m_cost >= {MIN_M_COST}, t_cost >= {MIN_T_COST}, p_cost >= {MIN_P_COST})")]
    WeakKdfParams,
}

impl BackupEnvelope {
    /// Parse and validate a raw envelope.
    ///
    /// # Errors
    ///
    /// Returns an error if the envelope is malformed, too small/large,
    /// or uses an unsupported version/KDF.
    pub fn parse(bytes: Vec<u8>) -> Result<Self, EnvelopeError> {
        if bytes.len() < MIN_ENVELOPE_SIZE {
            return Err(EnvelopeError::TooSmall);
        }
        if bytes.len() > MAX_ENVELOPE_SIZE {
            return Err(EnvelopeError::TooLarge);
        }
        if bytes[0] != VERSION {
            return Err(EnvelopeError::UnsupportedVersion);
        }
        if bytes[1] != KDF_ARGON2ID {
            return Err(EnvelopeError::UnsupportedKdf);
        }

        let m_cost = u32::from_le_bytes([bytes[2], bytes[3], bytes[4], bytes[5]]);
        let t_cost = u32::from_le_bytes([bytes[6], bytes[7], bytes[8], bytes[9]]);
        let p_cost = u32::from_le_bytes([bytes[10], bytes[11], bytes[12], bytes[13]]);

        if m_cost < MIN_M_COST || t_cost < MIN_T_COST || p_cost < MIN_P_COST {
            return Err(EnvelopeError::WeakKdfParams);
        }

        let mut salt = [0u8; 16];
        salt.copy_from_slice(&bytes[SALT_OFFSET..SALT_OFFSET + 16]);

        Ok(Self {
            salt,
            version: bytes[0],
            raw: bytes,
        })
    }

    /// Build an envelope from individual fields.
    ///
    /// Useful for tests and future frontend construction.
    ///
    /// # Errors
    ///
    /// Returns `EnvelopeError::CiphertextTooSmall` if ciphertext is under 48 bytes.
    /// Returns `EnvelopeError::TooLarge` if the assembled envelope exceeds 4096 bytes.
    pub fn build(
        salt: [u8; 16],
        m_cost: u32,
        t_cost: u32,
        p_cost: u32,
        nonce: [u8; 12],
        ciphertext: &[u8],
    ) -> Result<Self, EnvelopeError> {
        if ciphertext.len() < MIN_CIPHERTEXT {
            return Err(EnvelopeError::CiphertextTooSmall);
        }
        if m_cost < MIN_M_COST || t_cost < MIN_T_COST || p_cost < MIN_P_COST {
            return Err(EnvelopeError::WeakKdfParams);
        }
        let total = HEADER_SIZE + ciphertext.len();
        if total > MAX_ENVELOPE_SIZE {
            return Err(EnvelopeError::TooLarge);
        }

        let mut raw = Vec::with_capacity(total);
        raw.push(VERSION);
        raw.push(KDF_ARGON2ID);
        raw.extend_from_slice(&m_cost.to_le_bytes());
        raw.extend_from_slice(&t_cost.to_le_bytes());
        raw.extend_from_slice(&p_cost.to_le_bytes());
        raw.extend_from_slice(&salt);
        raw.extend_from_slice(&nonce);
        raw.extend_from_slice(ciphertext);

        Ok(Self {
            salt,
            version: VERSION,
            raw,
        })
    }

    /// The 16-byte KDF salt.
    #[must_use]
    pub const fn salt(&self) -> &[u8; 16] {
        &self.salt
    }

    /// Envelope version (currently always 1).
    #[must_use]
    pub fn version(&self) -> i32 {
        i32::from(self.version)
    }

    /// The raw envelope bytes (for storage).
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.raw
    }

    /// Consume and return the raw bytes.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.raw
    }
}

impl fmt::Debug for BackupEnvelope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BackupEnvelope")
            .field("version", &self.version)
            .field("size", &self.raw.len())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ciphertext() -> Vec<u8> {
        vec![0xCC; MIN_CIPHERTEXT]
    }

    #[test]
    fn build_and_parse_roundtrip() {
        let salt = [0xAA; 16];
        let nonce = [0xBB; 12];
        let ct = test_ciphertext();

        let envelope = BackupEnvelope::build(salt, 65536, 3, 1, nonce, &ct).expect("build");
        assert_eq!(envelope.salt(), &salt);
        assert_eq!(envelope.version(), 1);
        assert_eq!(envelope.as_bytes().len(), MIN_ENVELOPE_SIZE);

        // Re-parse the raw bytes
        let parsed = BackupEnvelope::parse(envelope.into_bytes()).expect("parse");
        assert_eq!(parsed.salt(), &salt);
        assert_eq!(parsed.version(), 1);
    }

    #[test]
    fn parse_rejects_too_small() {
        assert!(matches!(
            BackupEnvelope::parse(vec![0u8; 10]),
            Err(EnvelopeError::TooSmall)
        ));
    }

    #[test]
    fn parse_rejects_too_large() {
        assert!(matches!(
            BackupEnvelope::parse(vec![0u8; MAX_ENVELOPE_SIZE + 1]),
            Err(EnvelopeError::TooLarge)
        ));
    }

    #[test]
    fn parse_rejects_wrong_version() {
        let mut raw = vec![0u8; MIN_ENVELOPE_SIZE];
        raw[0] = 0x02; // bad version
        raw[1] = KDF_ARGON2ID;
        assert!(matches!(
            BackupEnvelope::parse(raw),
            Err(EnvelopeError::UnsupportedVersion)
        ));
    }

    #[test]
    fn parse_rejects_pbkdf2() {
        let mut raw = vec![0u8; MIN_ENVELOPE_SIZE];
        raw[0] = VERSION;
        raw[1] = 0x02; // PBKDF2
        assert!(matches!(
            BackupEnvelope::parse(raw),
            Err(EnvelopeError::UnsupportedKdf)
        ));
    }

    #[test]
    fn build_rejects_short_ciphertext() {
        assert!(matches!(
            BackupEnvelope::build([0; 16], 65536, 3, 1, [0; 12], &[0u8; 10]),
            Err(EnvelopeError::CiphertextTooSmall)
        ));
    }

    #[test]
    fn build_rejects_weak_kdf_params() {
        assert!(matches!(
            BackupEnvelope::build([0; 16], 1, 1, 1, [0; 12], &test_ciphertext()),
            Err(EnvelopeError::WeakKdfParams)
        ));
    }

    #[test]
    fn parse_rejects_weak_kdf_params() {
        let mut raw = vec![0u8; MIN_ENVELOPE_SIZE];
        raw[0] = VERSION;
        raw[1] = KDF_ARGON2ID;
        // m_cost = 1 (too weak)
        raw[2..6].copy_from_slice(&1u32.to_le_bytes());
        raw[6..10].copy_from_slice(&1u32.to_le_bytes());
        raw[10..14].copy_from_slice(&1u32.to_le_bytes());
        assert!(matches!(
            BackupEnvelope::parse(raw),
            Err(EnvelopeError::WeakKdfParams)
        ));
    }

    #[test]
    fn salt_extracted_correctly() {
        let salt = [0x42; 16];
        let envelope = BackupEnvelope::build(salt, 65536, 3, 1, [0xBB; 12], &test_ciphertext())
            .expect("build");
        assert_eq!(envelope.salt(), &salt);
    }
}
