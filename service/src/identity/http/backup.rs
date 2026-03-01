//! Backup retrieval endpoint for login/recovery flow.
//!
//! Returns the encrypted backup envelope for a given username. To prevent
//! username enumeration, unknown usernames receive a deterministic synthetic
//! backup that the client cannot distinguish from a real one until decryption
//! fails (indistinguishable from a wrong password).

use std::sync::Arc;

use axum::{extract::Extension, extract::Path, http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tc_crypto::Kid;

use crate::identity::repo::{AccountRepoError, BackupRepoError, IdentityRepo};

#[derive(Debug, Serialize)]
pub struct BackupResponse {
    pub encrypted_backup: String,
    pub root_kid: Kid,
}

/// Generate a deterministic synthetic backup for an unknown username.
///
/// Returns a valid-looking 90-byte backup envelope and a fake KID.
/// The envelope passes format validation but always fails at ChaCha20-Poly1305
/// decryption, making it indistinguishable from a wrong-password attempt.
fn synthetic_backup(username: &str) -> (Vec<u8>, Kid) {
    // Deterministic KID from username
    let kid_seed = Sha256::digest(format!("tc-kid-pad:{username}").as_bytes());
    let kid = Kid::derive(&kid_seed);

    let mut envelope = vec![0u8; 90];
    envelope[0] = 0x01; // version
    envelope[1] = 0x01; // kdf_id = Argon2id

    // Valid Argon2id parameters matching real envelopes
    envelope[2..6].copy_from_slice(&65536_u32.to_le_bytes()); // m_cost
    envelope[6..10].copy_from_slice(&3_u32.to_le_bytes()); // t_cost
    envelope[10..14].copy_from_slice(&1_u32.to_le_bytes()); // p_cost

    // Deterministic salt (16 bytes)
    let salt = Sha256::digest(format!("tc-salt-pad:{username}").as_bytes());
    envelope[14..30].copy_from_slice(&salt[..16]);

    // Deterministic nonce (12 bytes)
    let nonce = Sha256::digest(format!("tc-nonce-pad:{username}").as_bytes());
    envelope[30..42].copy_from_slice(&nonce[..12]);

    // Deterministic ciphertext (48 bytes)
    let ct1 = Sha256::digest(format!("tc-ct1-pad:{username}").as_bytes());
    let ct2 = Sha256::digest(format!("tc-ct2-pad:{username}").as_bytes());
    envelope[42..74].copy_from_slice(&ct1);
    envelope[74..90].copy_from_slice(&ct2[..16]);

    (envelope, kid)
}

/// GET /auth/backup/{username} -- fetch encrypted backup for login.
///
/// Returns 200 with an encrypted backup envelope for both real and unknown
/// usernames. Unknown usernames receive a deterministic synthetic backup
/// to prevent username enumeration.
pub async fn get_backup(
    Extension(repo): Extension<Arc<dyn IdentityRepo>>,
    Path(username): Path<String>,
) -> impl IntoResponse {
    let username = username.trim();
    if username.is_empty() {
        return super::bad_request("Username cannot be empty");
    }

    let account = match repo.get_account_by_username(username).await {
        Ok(a) => a,
        Err(AccountRepoError::NotFound) => {
            // Return synthetic backup to prevent username enumeration.
            // The client will fail at decryption, indistinguishable from wrong password.
            let (fake_backup, fake_kid) = synthetic_backup(username);
            return (
                StatusCode::OK,
                Json(BackupResponse {
                    encrypted_backup: tc_crypto::encode_base64url(&fake_backup),
                    root_kid: fake_kid,
                }),
            )
                .into_response();
        }
        Err(AccountRepoError::Database(e)) => {
            tracing::error!("Failed to look up account: {e}");
            return super::internal_error();
        }
        Err(e) => {
            tracing::error!("Unexpected error looking up account: {e}");
            return super::internal_error();
        }
    };

    match repo.get_backup_by_kid(&account.root_kid).await {
        Ok(record) => {
            let encrypted_backup = tc_crypto::encode_base64url(&record.encrypted_backup);
            (
                StatusCode::OK,
                Json(BackupResponse {
                    encrypted_backup,
                    root_kid: account.root_kid,
                }),
            )
                .into_response()
        }
        Err(BackupRepoError::NotFound) => {
            // Account exists but has no backup — return synthetic to avoid
            // leaking that the account exists without a backup.
            let (fake_backup, fake_kid) = synthetic_backup(username);
            (
                StatusCode::OK,
                Json(BackupResponse {
                    encrypted_backup: tc_crypto::encode_base64url(&fake_backup),
                    root_kid: fake_kid,
                }),
            )
                .into_response()
        }
        Err(BackupRepoError::Database(e)) => {
            tracing::error!("Failed to fetch backup: {e}");
            super::internal_error()
        }
        Err(e) => {
            tracing::error!("Unexpected error fetching backup: {e}");
            super::internal_error()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_backup_is_deterministic() {
        let (backup1, kid1) = synthetic_backup("alice");
        let (backup2, kid2) = synthetic_backup("alice");
        assert_eq!(backup1, backup2);
        assert_eq!(kid1, kid2);
    }

    #[test]
    fn synthetic_backup_differs_by_username() {
        let (backup_a, kid_a) = synthetic_backup("alice");
        let (backup_b, kid_b) = synthetic_backup("bob");
        assert_ne!(backup_a, backup_b);
        assert_ne!(kid_a, kid_b);
    }

    #[test]
    fn synthetic_backup_has_correct_size() {
        let (backup, _kid) = synthetic_backup("testuser");
        assert_eq!(backup.len(), 90);
    }

    #[test]
    fn synthetic_backup_has_valid_header() {
        let (backup, _kid) = synthetic_backup("testuser");
        assert_eq!(backup[0], 0x01); // version
        assert_eq!(backup[1], 0x01); // kdf_id = Argon2id
        let m_cost = u32::from_le_bytes([backup[2], backup[3], backup[4], backup[5]]);
        let t_cost = u32::from_le_bytes([backup[6], backup[7], backup[8], backup[9]]);
        let p_cost = u32::from_le_bytes([backup[10], backup[11], backup[12], backup[13]]);
        assert_eq!(m_cost, 65536);
        assert_eq!(t_cost, 3);
        assert_eq!(p_cost, 1);
    }
}
