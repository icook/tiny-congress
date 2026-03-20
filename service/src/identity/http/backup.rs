// lint-patterns:allow-no-utoipa — tracked by #906
//! Backup retrieval endpoint for login/recovery flow.
//!
//! Returns the encrypted backup envelope for a given username. To prevent
//! username enumeration, unknown usernames receive a deterministic synthetic
//! backup that the client cannot distinguish from a real one until decryption
//! fails (indistinguishable from a wrong password).
//!
//! Synthetic backups are keyed with a server-side HMAC secret so that external
//! observers cannot precompute the expected response for a given username.

use std::sync::Arc;

use axum::{extract::Extension, extract::Path, http::StatusCode, response::IntoResponse, Json};
use hmac::{Hmac, Mac};
use serde::Serialize;
use sha2::Sha256;
use tc_crypto::Kid;

use crate::identity::repo::{AccountRepoError, BackupRepoError, IdentityRepo};
use crate::identity::service::validate_username;

type HmacSha256 = Hmac<Sha256>;

/// Server-side HMAC key for generating synthetic backup envelopes.
///
/// Wrapped in a newtype so it can be passed as an axum Extension without
/// conflicting with other `Vec<u8>` or `String` extensions.
#[derive(Clone)]
pub struct SyntheticBackupKey(Vec<u8>);

impl SyntheticBackupKey {
    /// Minimum key length — HMAC-SHA256 recommends keys at least as long as the
    /// hash output (32 bytes). Shorter keys weaken the anti-enumeration guarantee
    /// because attackers could brute-force the key and precompute synthetic backups.
    pub const MIN_KEY_LEN: usize = 32;

    /// Create a new synthetic backup HMAC key.
    ///
    /// # Panics
    ///
    /// Panics if `key` is shorter than [`Self::MIN_KEY_LEN`] bytes.
    /// This is a startup-time configuration error that should prevent the
    /// server from running with a weak anti-enumeration key.
    #[must_use]
    pub fn new(key: Vec<u8>) -> Self {
        assert!(
            key.len() >= Self::MIN_KEY_LEN,
            "SyntheticBackupKey must be at least {} bytes, got {}",
            Self::MIN_KEY_LEN,
            key.len()
        );
        Self(key)
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Debug, Serialize)]
pub struct BackupResponse {
    pub encrypted_backup: String,
    pub root_kid: Kid,
}

/// Compute HMAC-SHA256(key, message) and return the 32-byte tag.
///
/// HMAC-SHA256 accepts keys of any length (RFC 2104 §2), so
/// `new_from_slice` cannot fail. The `Ok`-only match is a
/// compile-time-safe alternative to `expect`/`unwrap`.
fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let Ok(mut mac) = HmacSha256::new_from_slice(key) else {
        unreachable!("HMAC-SHA256 accepts any key length per RFC 2104")
    };
    mac.update(message);
    mac.finalize().into_bytes().into()
}

/// Generate a deterministic synthetic backup for an unknown username.
///
/// Returns a valid-looking 90-byte backup envelope and a fake KID.
/// The envelope passes format validation but always fails at ChaCha20-Poly1305
/// decryption, making it indistinguishable from a wrong-password attempt.
///
/// The output is keyed by `hmac_key` so that external observers cannot
/// precompute expected values for a given username.
fn synthetic_backup(username: &str, hmac_key: &[u8]) -> (Vec<u8>, Kid) {
    // Deterministic KID from username (keyed)
    let kid_seed = hmac_sha256(hmac_key, format!("tc-kid-pad:{username}").as_bytes());
    let kid = Kid::derive(&kid_seed);

    let mut envelope = vec![0u8; 90];
    envelope[0] = 0x01; // version
    envelope[1] = 0x01; // kdf_id = Argon2id

    // Valid Argon2id parameters matching real envelopes
    envelope[2..6].copy_from_slice(&65536_u32.to_le_bytes()); // m_cost
    envelope[6..10].copy_from_slice(&3_u32.to_le_bytes()); // t_cost
    envelope[10..14].copy_from_slice(&1_u32.to_le_bytes()); // p_cost

    // Deterministic salt (16 bytes)
    let salt = hmac_sha256(hmac_key, format!("tc-salt-pad:{username}").as_bytes());
    envelope[14..30].copy_from_slice(&salt[..16]);

    // Deterministic nonce (12 bytes)
    let nonce = hmac_sha256(hmac_key, format!("tc-nonce-pad:{username}").as_bytes());
    envelope[30..42].copy_from_slice(&nonce[..12]);

    // Deterministic ciphertext (48 bytes)
    let ct1 = hmac_sha256(hmac_key, format!("tc-ct1-pad:{username}").as_bytes());
    let ct2 = hmac_sha256(hmac_key, format!("tc-ct2-pad:{username}").as_bytes());
    envelope[42..74].copy_from_slice(&ct1);
    envelope[74..90].copy_from_slice(&ct2[..16]);

    (envelope, kid)
}

/// Build the 200 OK response containing a synthetic backup envelope.
///
/// Called for both "account not found" and "account found but no backup" cases.
/// A single function ensures both code paths produce structurally identical
/// responses, preserving the enumeration-protection guarantee.
fn synthetic_backup_response(username: &str, hmac_key: &[u8]) -> axum::response::Response {
    let (fake_backup, fake_kid) = synthetic_backup(username, hmac_key);
    (
        StatusCode::OK,
        Json(BackupResponse {
            encrypted_backup: tc_crypto::encode_base64url(&fake_backup),
            root_kid: fake_kid,
        }),
    )
        .into_response()
}

/// GET /auth/backup/{username} -- fetch encrypted backup for login.
///
/// Returns 200 with an encrypted backup envelope for both real and unknown
/// usernames. Unknown usernames receive a deterministic synthetic backup
/// to prevent username enumeration.
///
/// To mitigate timing side-channels, both the account lookup and backup
/// lookup are always performed regardless of whether the account exists.
pub async fn get_backup(
    Extension(repo): Extension<Arc<dyn IdentityRepo>>,
    Extension(hmac_key): Extension<SyntheticBackupKey>,
    Path(username): Path<String>,
) -> impl IntoResponse {
    let username = username.trim();
    if let Err(e) = validate_username(username) {
        return super::bad_request(&e.to_string());
    }

    // Always perform the account lookup.
    let account_result = repo.get_account_by_username(username).await;

    // Always perform a backup lookup to keep timing consistent.
    // For unknown users we use a random ephemeral KID so the DB lookup
    // timing matches a genuine first-time lookup.
    let dummy_kid = Kid::derive(&rand::random::<[u8; 32]>());
    let lookup_kid = account_result
        .as_ref()
        .map_or(&dummy_kid, |account| &account.root_kid);
    let backup_result = repo.get_backup_by_kid(lookup_kid).await;

    // Now branch on results.
    let account = match account_result {
        Ok(a) => a,
        Err(AccountRepoError::NotFound) => {
            return synthetic_backup_response(username, hmac_key.as_bytes());
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

    match backup_result {
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
            synthetic_backup_response(username, hmac_key.as_bytes())
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
    use crate::identity::repo::{
        mock::MockIdentityRepo, AccountRecord, AccountRepoError, BackupRecord, BackupRepoError,
    };
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
        routing::get,
        Router,
    };
    use std::sync::Arc;
    use tc_crypto::{encode_base64url, Kid};
    use tower::ServiceExt;
    use uuid::Uuid;

    const TEST_HMAC_KEY: &[u8] = b"test-hmac-key-for-unit-tests-pad";

    #[test]
    fn synthetic_backup_key_accepts_32_byte_key() {
        let key = SyntheticBackupKey::new(vec![0xAA; 32]);
        assert_eq!(key.as_bytes().len(), 32);
    }

    #[test]
    #[should_panic(expected = "must be at least 32 bytes")]
    fn synthetic_backup_key_rejects_empty_key() {
        let _ = SyntheticBackupKey::new(vec![]);
    }

    #[test]
    #[should_panic(expected = "must be at least 32 bytes")]
    fn synthetic_backup_key_rejects_short_key() {
        let _ = SyntheticBackupKey::new(vec![0xAA; 16]);
    }

    #[test]
    fn synthetic_backup_is_deterministic() {
        let (backup1, kid1) = synthetic_backup("alice", TEST_HMAC_KEY);
        let (backup2, kid2) = synthetic_backup("alice", TEST_HMAC_KEY);
        assert_eq!(backup1, backup2);
        assert_eq!(kid1, kid2);
    }

    #[test]
    fn synthetic_backup_differs_by_username() {
        let (backup_a, kid_a) = synthetic_backup("alice", TEST_HMAC_KEY);
        let (backup_b, kid_b) = synthetic_backup("bob", TEST_HMAC_KEY);
        assert_ne!(backup_a, backup_b);
        assert_ne!(kid_a, kid_b);
    }

    #[test]
    fn synthetic_backup_differs_by_key() {
        let (backup_a, kid_a) = synthetic_backup("alice", b"key-one");
        let (backup_b, kid_b) = synthetic_backup("alice", b"key-two");
        assert_ne!(backup_a, backup_b);
        assert_ne!(kid_a, kid_b);
    }

    #[test]
    fn synthetic_backup_has_correct_size() {
        let (backup, _kid) = synthetic_backup("testuser", TEST_HMAC_KEY);
        assert_eq!(backup.len(), 90);
    }

    #[test]
    fn synthetic_backup_has_valid_header() {
        let (backup, _kid) = synthetic_backup("testuser", TEST_HMAC_KEY);
        assert_eq!(backup[0], 0x01); // version
        assert_eq!(backup[1], 0x01); // kdf_id = Argon2id
        let m_cost = u32::from_le_bytes([backup[2], backup[3], backup[4], backup[5]]);
        let t_cost = u32::from_le_bytes([backup[6], backup[7], backup[8], backup[9]]);
        let p_cost = u32::from_le_bytes([backup[10], backup[11], backup[12], backup[13]]);
        assert_eq!(m_cost, 65536);
        assert_eq!(t_cost, 3);
        assert_eq!(p_cost, 1);
    }

    /// The synthetic backup must pass `BackupEnvelope::parse` successfully.
    ///
    /// The anti-enumeration guarantee requires that the synthetic backup be
    /// indistinguishable from a real backup until ChaCha20-Poly1305 decryption
    /// fails (wrong password). If `BackupEnvelope::parse` rejects the synthetic
    /// envelope, the client could distinguish "unknown user" from "wrong password"
    /// at the parse step — breaking enumeration protection. This test enforces
    /// that invariant so that tightened envelope validation rules don't silently
    /// break it.
    #[test]
    fn synthetic_backup_is_parseable_as_valid_envelope() {
        let (backup_bytes, _kid) = synthetic_backup("testuser", TEST_HMAC_KEY);
        assert!(
            tc_crypto::BackupEnvelope::parse(backup_bytes).is_ok(),
            "synthetic backup must pass envelope parsing to preserve anti-enumeration invariant"
        );
    }

    // ── Handler-level tests ────────────────────────────────────────────────

    fn test_router(repo: MockIdentityRepo) -> Router {
        Router::new()
            .route("/auth/backup/{username}", get(get_backup))
            .layer(axum::extract::Extension(
                Arc::new(repo) as Arc<dyn crate::identity::repo::IdentityRepo>
            ))
            .layer(axum::extract::Extension(SyntheticBackupKey::new(
                TEST_HMAC_KEY.to_vec(),
            )))
    }

    fn backup_request(username: &str) -> Request<Body> {
        Request::builder()
            .method("GET")
            .uri(format!("/auth/backup/{username}"))
            .body(Body::empty())
            .expect("request builder")
    }

    /// Whitespace-only usernames trim to empty and must be rejected before any DB lookup.
    #[tokio::test]
    async fn test_get_backup_whitespace_username_returns_bad_request() {
        let repo = MockIdentityRepo::new();
        let app = test_router(repo);

        // %20 decodes to a space; trim() makes it empty, triggering the early check.
        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/auth/backup/%20")
                    .body(Body::empty())
                    .expect("request builder"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(response.into_body(), 1024).await.expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert!(payload["error"].as_str().unwrap().contains("empty"));
    }

    #[tokio::test]
    async fn test_get_backup_too_long_username_returns_bad_request() {
        let repo = MockIdentityRepo::new();
        let app = test_router(repo);

        let username = "a".repeat(65);
        let response = app
            .oneshot(backup_request(&username))
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    /// Anti-enumeration: unknown username must return 200, not 404.
    #[tokio::test]
    async fn test_get_backup_unknown_user_returns_200() {
        let repo = MockIdentityRepo::new(); // default: account lookup returns NotFound
        let app = test_router(repo);

        let response = app
            .oneshot(backup_request("unknown-user"))
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert!(payload["encrypted_backup"].as_str().is_some());
        assert!(payload["root_kid"].as_str().is_some());
    }

    /// Anti-enumeration: account with no backup must return 200 synthetic, not 404.
    #[tokio::test]
    async fn test_get_backup_account_without_backup_returns_200_synthetic() {
        let repo = MockIdentityRepo::new();
        repo.set_account_by_username_result(Ok(AccountRecord {
            id: Uuid::new_v4(),
            username: "alice".to_string(),
            root_pubkey: encode_base64url(&[1u8; 32]),
            root_kid: Kid::derive(&[1u8; 32]),
        }));
        // backup lookup defaults to NotFound
        let app = test_router(repo);

        let response = app
            .oneshot(backup_request("alice"))
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert!(payload["encrypted_backup"].as_str().is_some());
    }

    /// Anti-enumeration: account with no backup must not leak the real root_kid.
    ///
    /// If the handler returned the real account's root_kid alongside a synthetic backup,
    /// an attacker could confirm that a username is registered (just without a backup)
    /// by cross-referencing the root_kid across calls.
    #[tokio::test]
    async fn test_get_backup_account_without_backup_does_not_leak_root_kid() {
        let real_root_kid = Kid::derive(&[1u8; 32]);
        let repo = MockIdentityRepo::new();
        repo.set_account_by_username_result(Ok(AccountRecord {
            id: Uuid::new_v4(),
            username: "alice".to_string(),
            root_pubkey: encode_base64url(&[1u8; 32]),
            root_kid: real_root_kid.clone(),
        }));
        // backup lookup defaults to NotFound
        let app = test_router(repo);

        let response = app
            .oneshot(backup_request("alice"))
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("json");
        let returned_kid = payload["root_kid"].as_str().expect("root_kid field");
        assert_ne!(
            returned_kid,
            real_root_kid.as_str(),
            "real root_kid must not be returned when account has no backup"
        );
    }

    /// Happy path: real backup returned for known user.
    #[tokio::test]
    async fn test_get_backup_known_user_with_backup_returns_real_backup() {
        let root_kid = Kid::derive(&[2u8; 32]);
        let encrypted_backup = vec![0xAA; 90];

        let repo = MockIdentityRepo::new();
        repo.set_account_by_username_result(Ok(AccountRecord {
            id: Uuid::new_v4(),
            username: "alice".to_string(),
            root_pubkey: encode_base64url(&[2u8; 32]),
            root_kid: root_kid.clone(),
        }));
        repo.set_get_backup_by_kid_result(Ok(BackupRecord {
            id: Uuid::new_v4(),
            account_id: Uuid::new_v4(),
            kid: root_kid.clone(),
            encrypted_backup: encrypted_backup.clone(),
            salt: vec![0; 16],
            version: 1,
            created_at: chrono::Utc::now(),
        }));
        let app = test_router(repo);

        let response = app
            .oneshot(backup_request("alice"))
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(
            payload["encrypted_backup"].as_str().unwrap(),
            encode_base64url(&encrypted_backup)
        );
        assert_eq!(payload["root_kid"].as_str().unwrap(), root_kid.as_str());
    }

    /// DB error on account lookup returns 500 (not a synthetic backup).
    #[tokio::test]
    async fn test_get_backup_account_db_error_returns_500() {
        let repo = MockIdentityRepo::new();
        repo.set_account_by_username_result(Err(AccountRepoError::Database(
            sqlx::Error::Protocol("db error".to_string()),
        )));
        let app = test_router(repo);

        let response = app
            .oneshot(backup_request("alice"))
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    /// DB error on backup lookup returns 500 (not a synthetic backup).
    #[tokio::test]
    async fn test_get_backup_backup_db_error_returns_500() {
        let repo = MockIdentityRepo::new();
        repo.set_account_by_username_result(Ok(AccountRecord {
            id: Uuid::new_v4(),
            username: "alice".to_string(),
            root_pubkey: encode_base64url(&[3u8; 32]),
            root_kid: Kid::derive(&[3u8; 32]),
        }));
        repo.set_get_backup_by_kid_result(Err(BackupRepoError::Database(sqlx::Error::Protocol(
            "db error".to_string(),
        ))));
        let app = test_router(repo);

        let response = app
            .oneshot(backup_request("alice"))
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
