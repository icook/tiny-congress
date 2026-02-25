//! HTTP handlers for identity system

use std::sync::Arc;

use axum::{
    extract::Extension, http::StatusCode, response::IntoResponse, routing::post, Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::repo::{
    AccountRepo, AccountRepoError, BackupRepo, BackupRepoError, DeviceKeyRepo, DeviceKeyRepoError,
};
use tc_crypto::{decode_base64url_native as decode_base64url, derive_kid};

/// Backup data included in signup request
#[derive(Debug, Deserialize)]
pub struct SignupBackup {
    /// Base64url-encoded encrypted backup envelope
    pub encrypted_blob: String,
}

/// Device data included in signup request
#[derive(Debug, Deserialize)]
pub struct SignupDevice {
    /// Base64url-encoded Ed25519 public key
    pub pubkey: String,
    /// User-provided device name
    pub name: String,
    /// Base64url-encoded certificate (root key's signature over canonical cert message)
    pub certificate: String,
}

/// Signup request payload — atomic creation of account + backup + first device
#[derive(Debug, Deserialize)]
pub struct SignupRequest {
    pub username: String,
    pub root_pubkey: String, // base64url encoded
    pub backup: SignupBackup,
    pub device: SignupDevice,
}

/// Signup response
#[derive(Debug, Serialize, Deserialize)]
pub struct SignupResponse {
    pub account_id: Uuid,
    pub root_kid: String,
    pub device_kid: String,
}

/// Error response
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
}

const RESERVED_USERNAMES: &[&str] = &[
    "admin",
    "administrator",
    "root",
    "system",
    "mod",
    "moderator",
    "support",
    "help",
    "api",
    "graphql",
    "auth",
    "signup",
    "login",
    "null",
    "undefined",
    "anonymous",
];

/// Validate a username, returning an error message if invalid.
fn validate_username(username: &str) -> Result<(), &'static str> {
    if username.is_empty() {
        return Err("Username cannot be empty");
    }
    if username.len() < 3 {
        return Err("Username must be at least 3 characters");
    }
    if username.len() > 64 {
        return Err("Username too long");
    }
    if !username
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err("Username may only contain letters, numbers, hyphens, and underscores");
    }
    if RESERVED_USERNAMES.contains(&username.to_ascii_lowercase().as_str()) {
        return Err("This username is reserved");
    }
    Ok(())
}

/// Create identity router
pub fn router() -> Router {
    Router::new().route("/auth/signup", post(signup))
}

/// Minimum valid envelope size in bytes (version + KDF ID + KDF params + salt + nonce + ciphertext).
const MIN_ENVELOPE_SIZE: usize = 82;

/// Parse and validate the encrypted backup envelope, extracting the salt and KDF info.
/// Returns (version, KDF algorithm name, salt).
fn parse_envelope(bytes: &[u8]) -> Result<(i32, &'static str, Vec<u8>), &'static str> {
    if bytes.len() < MIN_ENVELOPE_SIZE {
        return Err("Encrypted backup envelope too small");
    }

    let version = bytes[0];
    if version != 1 {
        return Err("Unsupported backup envelope version");
    }

    let kdf_id = bytes[1];
    let kdf_algorithm = match kdf_id {
        1 => "argon2id",
        2 => "pbkdf2",
        _ => return Err("Unknown KDF algorithm in backup envelope"),
    };

    // Salt offset depends on KDF params size:
    // Argon2: params at bytes 2-13 (12 bytes: m:4, t:4, p:4), salt at 14-29
    // PBKDF2: params at bytes 2-5 (4 bytes: iterations), salt at 6-21
    let salt_offset = if kdf_id == 1 { 14 } else { 6 };
    let salt = bytes[salt_offset..salt_offset + 16].to_vec();

    Ok((i32::from(version), kdf_algorithm, salt))
}

/// Handle signup request — atomic creation of account + backup + first device key
async fn signup(
    Extension(account_repo): Extension<Arc<dyn AccountRepo>>,
    Extension(backup_repo): Extension<Arc<dyn BackupRepo>>,
    Extension(device_key_repo): Extension<Arc<dyn DeviceKeyRepo>>,
    Json(req): Json<SignupRequest>,
) -> impl IntoResponse {
    // Validate username
    let username = req.username.trim();
    if let Err(msg) = validate_username(username) {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: msg.to_string(),
            }),
        )
            .into_response();
    }

    // Decode and validate root public key
    let Ok(root_pubkey_bytes) = decode_base64url(&req.root_pubkey) else {
        return bad_request("Invalid base64url encoding for root_pubkey");
    };

    if root_pubkey_bytes.len() != 32 {
        return bad_request("root_pubkey must be 32 bytes (Ed25519)");
    }

    // Derive root KID
    let root_kid = derive_kid(&root_pubkey_bytes);

    // Decode and validate encrypted backup
    let Ok(backup_bytes) = decode_base64url(&req.backup.encrypted_blob) else {
        return bad_request("Invalid base64url encoding for backup.encrypted_blob");
    };

    let (version, kdf_algorithm, salt) = match parse_envelope(&backup_bytes) {
        Ok(parsed) => parsed,
        Err(msg) => return bad_request(msg),
    };

    // Decode and validate device public key
    let Ok(device_pubkey_bytes) = decode_base64url(&req.device.pubkey) else {
        return bad_request("Invalid base64url encoding for device.pubkey");
    };

    if device_pubkey_bytes.len() != 32 {
        return bad_request("device.pubkey must be 32 bytes (Ed25519)");
    }

    // Derive device KID
    let device_kid = derive_kid(&device_pubkey_bytes);

    // Validate device name
    let device_name = req.device.name.trim();
    if device_name.is_empty() {
        return bad_request("Device name cannot be empty");
    }

    if device_name.len() > 128 {
        return bad_request("Device name too long");
    }

    // Decode certificate
    let Ok(certificate_bytes) = decode_base64url(&req.device.certificate) else {
        return bad_request("Invalid base64url encoding for device.certificate");
    };

    if certificate_bytes.len() != 64 {
        return bad_request("device.certificate must be 64 bytes (Ed25519 signature)");
    }

    // Create account, backup, and device key.
    // Note: These are currently separate calls. For true atomicity, we'd use a
    // database transaction. For now, we create in order and accept that a partial
    // failure leaves orphan rows (which CASCADE delete will clean up if the
    // account creation itself failed). This is acceptable for PoC.
    //
    // TODO: Wrap in a transaction for production.
    let account = match account_repo
        .create(username, &req.root_pubkey, &root_kid)
        .await
    {
        Ok(account) => account,
        Err(e) => return account_error_response(e),
    };

    if let Err(e) = backup_repo
        .create(
            account.id,
            &root_kid,
            &backup_bytes,
            &salt,
            kdf_algorithm,
            version,
        )
        .await
    {
        return backup_error_response(e);
    }

    let device = match device_key_repo
        .create(
            account.id,
            &device_kid,
            &req.device.pubkey,
            device_name,
            &certificate_bytes,
        )
        .await
    {
        Ok(device) => device,
        Err(e) => return device_key_error_response(e),
    };

    (
        StatusCode::CREATED,
        Json(SignupResponse {
            account_id: account.id,
            root_kid: account.root_kid,
            device_kid: device.device_kid,
        }),
    )
        .into_response()
}

fn bad_request(msg: &str) -> axum::response::Response {
    (
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
            error: msg.to_string(),
        }),
    )
        .into_response()
}

fn account_error_response(e: AccountRepoError) -> axum::response::Response {
    match e {
        AccountRepoError::DuplicateUsername => (
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: "Username already taken".to_string(),
            }),
        )
            .into_response(),
        AccountRepoError::DuplicateKey => (
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: "Public key already registered".to_string(),
            }),
        )
            .into_response(),
        AccountRepoError::Database(db_err) => {
            tracing::error!("Signup failed (account): {}", db_err);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Internal server error".to_string(),
                }),
            )
                .into_response()
        }
    }
}

fn backup_error_response(e: BackupRepoError) -> axum::response::Response {
    match e {
        BackupRepoError::DuplicateAccount | BackupRepoError::DuplicateKid => (
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: "Backup already exists".to_string(),
            }),
        )
            .into_response(),
        BackupRepoError::NotFound => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Backup not found".to_string(),
            }),
        )
            .into_response(),
        BackupRepoError::Database(db_err) => {
            tracing::error!("Signup failed (backup): {}", db_err);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Internal server error".to_string(),
                }),
            )
                .into_response()
        }
    }
}

fn device_key_error_response(e: DeviceKeyRepoError) -> axum::response::Response {
    match e {
        DeviceKeyRepoError::DuplicateKid => (
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: "Device key already registered".to_string(),
            }),
        )
            .into_response(),
        DeviceKeyRepoError::MaxDevicesReached => (
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: "Maximum device limit reached".to_string(),
            }),
        )
            .into_response(),
        DeviceKeyRepoError::NotFound => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Device key not found".to_string(),
            }),
        )
            .into_response(),
        DeviceKeyRepoError::Database(db_err) => {
            tracing::error!("Signup failed (device key): {}", db_err);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Internal server error".to_string(),
                }),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::repo::mock::{MockAccountRepo, MockBackupRepo, MockDeviceKeyRepo};
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use tc_crypto::encode_base64url;
    use tower::ServiceExt;

    fn test_router(
        account_repo: Arc<dyn AccountRepo>,
        backup_repo: Arc<dyn BackupRepo>,
        device_key_repo: Arc<dyn DeviceKeyRepo>,
    ) -> Router {
        Router::new()
            .route("/auth/signup", post(signup))
            .layer(Extension(account_repo))
            .layer(Extension(backup_repo))
            .layer(Extension(device_key_repo))
    }

    fn default_repos() -> (
        Arc<MockAccountRepo>,
        Arc<MockBackupRepo>,
        Arc<MockDeviceKeyRepo>,
    ) {
        (
            Arc::new(MockAccountRepo::new()),
            Arc::new(MockBackupRepo::new()),
            Arc::new(MockDeviceKeyRepo::new()),
        )
    }

    /// Build a valid encrypted backup envelope (minimal valid structure).
    /// Argon2id envelope: version(1) + kdf_id(1) + params(12) + salt(16) + nonce(12) + ciphertext(48) = 90 bytes
    fn fake_backup_envelope() -> Vec<u8> {
        let mut envelope = Vec::with_capacity(90);
        envelope.push(0x01); // version
        envelope.push(0x01); // kdf_id = argon2id
        envelope.extend_from_slice(&[0u8; 12]); // kdf params (m, t, p)
        envelope.extend_from_slice(&[0xAA; 16]); // salt
        envelope.extend_from_slice(&[0xBB; 12]); // nonce
        envelope.extend_from_slice(&[0xCC; 48]); // ciphertext
        envelope
    }

    fn valid_signup_body() -> String {
        let root_pubkey = encode_base64url(&[1u8; 32]);
        let device_pubkey = encode_base64url(&[2u8; 32]);
        let certificate = encode_base64url(&[3u8; 64]);
        let backup_blob = encode_base64url(&fake_backup_envelope());

        format!(
            r#"{{
                "username": "alice",
                "root_pubkey": "{root_pubkey}",
                "backup": {{
                    "encrypted_blob": "{backup_blob}"
                }},
                "device": {{
                    "pubkey": "{device_pubkey}",
                    "name": "Test Device",
                    "certificate": "{certificate}"
                }}
            }}"#
        )
    }

    fn signup_request(body: &str) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri("/auth/signup")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .expect("request builder")
    }

    #[tokio::test]
    async fn test_signup_success() {
        let (account_repo, backup_repo, device_repo) = default_repos();
        let app = test_router(account_repo, backup_repo, device_repo);

        let response = app
            .oneshot(signup_request(&valid_signup_body()))
            .await
            .expect("response");

        let (parts, body) = response.into_parts();
        assert_eq!(parts.status, StatusCode::CREATED);

        let body_bytes = to_bytes(body, 1024 * 1024).await.expect("body bytes");
        let payload: SignupResponse = serde_json::from_slice(&body_bytes).expect("json payload");

        assert!(!payload.root_kid.is_empty());
        assert!(!payload.device_kid.is_empty());
    }

    #[tokio::test]
    async fn test_signup_empty_username() {
        let (account_repo, backup_repo, device_repo) = default_repos();
        let app = test_router(account_repo, backup_repo, device_repo);

        let body = valid_signup_body().replace("alice", "");
        let response = app.oneshot(signup_request(&body)).await.expect("response");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_signup_invalid_root_pubkey() {
        let (account_repo, backup_repo, device_repo) = default_repos();
        let app = test_router(account_repo, backup_repo, device_repo);

        let body = valid_signup_body().replace(&encode_base64url(&[1u8; 32]), "!!!not-base64!!!");
        let response = app.oneshot(signup_request(&body)).await.expect("response");

        let (parts, body) = response.into_parts();
        assert_eq!(parts.status, StatusCode::BAD_REQUEST);
        let body_bytes = to_bytes(body, 1024 * 1024).await.expect("body bytes");
        let payload: ErrorResponse = serde_json::from_slice(&body_bytes).expect("json payload");
        assert!(payload.error.contains("root_pubkey"));
    }

    #[tokio::test]
    async fn test_signup_short_root_pubkey() {
        let (account_repo, backup_repo, device_repo) = default_repos();
        let app = test_router(account_repo, backup_repo, device_repo);

        let body = valid_signup_body()
            .replace(&encode_base64url(&[1u8; 32]), &encode_base64url(&[1u8; 4]));
        let response = app.oneshot(signup_request(&body)).await.expect("response");

        let (parts, body) = response.into_parts();
        assert_eq!(parts.status, StatusCode::BAD_REQUEST);
        let body_bytes = to_bytes(body, 1024 * 1024).await.expect("body bytes");
        let payload: ErrorResponse = serde_json::from_slice(&body_bytes).expect("json payload");
        assert!(payload.error.contains("32 bytes"));
    }

    #[tokio::test]
    async fn test_signup_invalid_backup_envelope() {
        let (account_repo, backup_repo, device_repo) = default_repos();
        let app = test_router(account_repo, backup_repo, device_repo);

        // Too-short envelope
        let short_envelope = encode_base64url(&[0u8; 10]);
        let body = valid_signup_body()
            .replace(&encode_base64url(&fake_backup_envelope()), &short_envelope);
        let response = app.oneshot(signup_request(&body)).await.expect("response");

        let (parts, body) = response.into_parts();
        assert_eq!(parts.status, StatusCode::BAD_REQUEST);
        let body_bytes = to_bytes(body, 1024 * 1024).await.expect("body bytes");
        let payload: ErrorResponse = serde_json::from_slice(&body_bytes).expect("json payload");
        assert!(payload.error.contains("envelope"));
    }

    #[tokio::test]
    async fn test_signup_invalid_device_pubkey() {
        let (account_repo, backup_repo, device_repo) = default_repos();
        let app = test_router(account_repo, backup_repo, device_repo);

        let body = valid_signup_body()
            .replace(&encode_base64url(&[2u8; 32]), &encode_base64url(&[2u8; 16]));
        let response = app.oneshot(signup_request(&body)).await.expect("response");

        let (parts, body) = response.into_parts();
        assert_eq!(parts.status, StatusCode::BAD_REQUEST);
        let body_bytes = to_bytes(body, 1024 * 1024).await.expect("body bytes");
        let payload: ErrorResponse = serde_json::from_slice(&body_bytes).expect("json payload");
        assert!(payload.error.contains("device.pubkey"));
    }

    #[tokio::test]
    async fn test_signup_empty_device_name() {
        let (account_repo, backup_repo, device_repo) = default_repos();
        let app = test_router(account_repo, backup_repo, device_repo);

        let body = valid_signup_body().replace("Test Device", "");
        let response = app.oneshot(signup_request(&body)).await.expect("response");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_signup_invalid_certificate_length() {
        let (account_repo, backup_repo, device_repo) = default_repos();
        let app = test_router(account_repo, backup_repo, device_repo);

        let body = valid_signup_body()
            .replace(&encode_base64url(&[3u8; 64]), &encode_base64url(&[3u8; 32]));
        let response = app.oneshot(signup_request(&body)).await.expect("response");

        let (parts, body) = response.into_parts();
        assert_eq!(parts.status, StatusCode::BAD_REQUEST);
        let body_bytes = to_bytes(body, 1024 * 1024).await.expect("body bytes");
        let payload: ErrorResponse = serde_json::from_slice(&body_bytes).expect("json payload");
        assert!(payload.error.contains("certificate"));
    }

    #[tokio::test]
    async fn test_signup_duplicate_username() {
        let (account_repo, backup_repo, device_repo) = default_repos();
        account_repo.set_create_result(Err(AccountRepoError::DuplicateUsername));
        let app = test_router(account_repo, backup_repo, device_repo);

        let response = app
            .oneshot(signup_request(&valid_signup_body()))
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn test_signup_duplicate_key() {
        let (account_repo, backup_repo, device_repo) = default_repos();
        account_repo.set_create_result(Err(AccountRepoError::DuplicateKey));
        let app = test_router(account_repo, backup_repo, device_repo);

        let response = app
            .oneshot(signup_request(&valid_signup_body()))
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn test_signup_username_too_short() {
        let mock_repo = Arc::new(MockAccountRepo::new());
        let app = test_router(mock_repo);

        let (root_pubkey, _) = encoded_pubkey(10);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/signup")
                    .header("content-type", "application/json")
                    .body(Body::from(format!(
                        r#"{{"username": "ab", "root_pubkey": "{root_pubkey}"}}"#
                    )))
                    .expect("request builder"),
            )
            .await
            .expect("response");

        let (parts, body) = response.into_parts();
        assert_eq!(parts.status, StatusCode::BAD_REQUEST);
        let body_bytes = to_bytes(body, 1024 * 1024).await.expect("body bytes");
        let payload: ErrorResponse = serde_json::from_slice(&body_bytes).expect("json payload");
        assert!(payload.error.contains("at least 3 characters"));
    }

    #[tokio::test]
    async fn test_signup_username_invalid_chars() {
        let mock_repo = Arc::new(MockAccountRepo::new());
        let app = test_router(mock_repo);

        let (root_pubkey, _) = encoded_pubkey(11);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/signup")
                    .header("content-type", "application/json")
                    .body(Body::from(format!(
                        r#"{{"username": "al!ce", "root_pubkey": "{root_pubkey}"}}"#
                    )))
                    .expect("request builder"),
            )
            .await
            .expect("response");

        let (parts, body) = response.into_parts();
        assert_eq!(parts.status, StatusCode::BAD_REQUEST);
        let body_bytes = to_bytes(body, 1024 * 1024).await.expect("body bytes");
        let payload: ErrorResponse = serde_json::from_slice(&body_bytes).expect("json payload");
        assert!(payload
            .error
            .contains("letters, numbers, hyphens, and underscores"));
    }

    #[tokio::test]
    async fn test_signup_username_unicode_rejected() {
        let mock_repo = Arc::new(MockAccountRepo::new());
        let app = test_router(mock_repo);

        let (root_pubkey, _) = encoded_pubkey(12);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/signup")
                    .header("content-type", "application/json")
                    .body(Body::from(format!(
                        r#"{{"username": "álice", "root_pubkey": "{root_pubkey}"}}"#
                    )))
                    .expect("request builder"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_signup_username_spaces_rejected() {
        let mock_repo = Arc::new(MockAccountRepo::new());
        let app = test_router(mock_repo);

        let (root_pubkey, _) = encoded_pubkey(13);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/signup")
                    .header("content-type", "application/json")
                    .body(Body::from(format!(
                        r#"{{"username": "al ice", "root_pubkey": "{root_pubkey}"}}"#
                    )))
                    .expect("request builder"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_signup_username_reserved() {
        let mock_repo = Arc::new(MockAccountRepo::new());
        let app = test_router(mock_repo);

        let (root_pubkey, _) = encoded_pubkey(14);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/signup")
                    .header("content-type", "application/json")
                    .body(Body::from(format!(
                        r#"{{"username": "admin", "root_pubkey": "{root_pubkey}"}}"#
                    )))
                    .expect("request builder"),
            )
            .await
            .expect("response");

        let (parts, body) = response.into_parts();
        assert_eq!(parts.status, StatusCode::BAD_REQUEST);
        let body_bytes = to_bytes(body, 1024 * 1024).await.expect("body bytes");
        let payload: ErrorResponse = serde_json::from_slice(&body_bytes).expect("json payload");
        assert!(payload.error.contains("reserved"));
    }

    #[tokio::test]
    async fn test_signup_username_reserved_case_insensitive() {
        let mock_repo = Arc::new(MockAccountRepo::new());
        let app = test_router(mock_repo);

        let (root_pubkey, _) = encoded_pubkey(15);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/signup")
                    .header("content-type", "application/json")
                    .body(Body::from(format!(
                        r#"{{"username": "Admin", "root_pubkey": "{root_pubkey}"}}"#
                    )))
                    .expect("request builder"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_signup_username_min_valid_length() {
        let mock_repo = Arc::new(MockAccountRepo::new());
        let app = test_router(mock_repo);

        let (root_pubkey, _) = encoded_pubkey(16);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/signup")
                    .header("content-type", "application/json")
                    .body(Body::from(format!(
                        r#"{{"username": "abc", "root_pubkey": "{root_pubkey}"}}"#
                    )))
                    .expect("request builder"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn test_signup_username_hyphens_underscores_valid() {
        let mock_repo = Arc::new(MockAccountRepo::new());
        let app = test_router(mock_repo);

        let (root_pubkey, _) = encoded_pubkey(17);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/signup")
                    .header("content-type", "application/json")
                    .body(Body::from(format!(
                        r#"{{"username": "a-b_c", "root_pubkey": "{root_pubkey}"}}"#
                    )))
                    .expect("request builder"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn test_signup_database_error_returns_500() {
        let (account_repo, backup_repo, device_repo) = default_repos();
        account_repo.set_create_result(Err(AccountRepoError::Database(sqlx::Error::Io(
            std::io::Error::new(std::io::ErrorKind::Other, "boom"),
        ))));
        let app = test_router(account_repo, backup_repo, device_repo);

        let response = app
            .oneshot(signup_request(&valid_signup_body()))
            .await
            .expect("response");

        let (parts, body) = response.into_parts();
        assert_eq!(parts.status, StatusCode::INTERNAL_SERVER_ERROR);

        let body_bytes = to_bytes(body, 1024 * 1024).await.expect("body bytes");
        let payload: ErrorResponse = serde_json::from_slice(&body_bytes).expect("json payload");
        assert!(payload.error.contains("Internal server error"));
    }
}
