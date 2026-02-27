//! HTTP handlers for identity system

pub mod auth;
pub mod backup;
pub mod devices;
pub mod login;
pub mod nonce;

use axum::{
    extract::Extension,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use super::repo::{
    create_account_with_executor, create_backup_with_executor, create_device_key_with_executor,
    AccountRepoError, BackupRepoError, DeviceKeyRepoError,
};
use tc_crypto::{decode_base64url_native as decode_base64url, verify_ed25519, BackupEnvelope, Kid};

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
    pub root_kid: Kid,
    pub device_kid: Kid,
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
    Router::new()
        .route("/auth/signup", post(signup))
        .route("/auth/backup/{username}", get(backup::get_backup))
        .route("/auth/login", post(login::login))
        .route(
            "/auth/devices",
            get(devices::list_devices).post(devices::add_device),
        )
        .route(
            "/auth/devices/{kid}",
            axum::routing::delete(devices::revoke_device).patch(devices::rename_device),
        )
}

/// Handle signup request — atomic creation of account + backup + first device key
async fn signup(
    Extension(pool): Extension<PgPool>,
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

    let Ok(root_pubkey_arr): Result<[u8; 32], _> = root_pubkey_bytes.as_slice().try_into() else {
        return bad_request("root_pubkey must be 32 bytes (Ed25519)");
    };

    // Derive root KID
    let root_kid = Kid::derive(&root_pubkey_arr);

    // Decode and validate encrypted backup
    let Ok(backup_bytes) = decode_base64url(&req.backup.encrypted_blob) else {
        return bad_request("Invalid base64url encoding for backup.encrypted_blob");
    };

    let envelope = match BackupEnvelope::parse(backup_bytes) {
        Ok(env) => env,
        Err(e) => return bad_request(&e.to_string()),
    };

    // Decode and validate device public key
    let Ok(device_pubkey_bytes) = decode_base64url(&req.device.pubkey) else {
        return bad_request("Invalid base64url encoding for device.pubkey");
    };

    if device_pubkey_bytes.len() != 32 {
        return bad_request("device.pubkey must be 32 bytes (Ed25519)");
    }

    // Derive device KID
    let device_kid = Kid::derive(&device_pubkey_bytes);

    // Validate device name
    let device_name = req.device.name.trim();
    if device_name.is_empty() {
        return bad_request("Device name cannot be empty");
    }

    if device_name.chars().count() > 128 {
        return bad_request("Device name too long");
    }

    // Decode certificate
    let Ok(certificate_bytes) = decode_base64url(&req.device.certificate) else {
        return bad_request("Invalid base64url encoding for device.certificate");
    };

    let Ok(cert_arr): Result<[u8; 64], _> = certificate_bytes.as_slice().try_into() else {
        return bad_request("device.certificate must be 64 bytes (Ed25519 signature)");
    };

    // Verify the certificate: root key must have signed the device public key.
    // The signed message is the raw 32-byte device pubkey. This is sufficient because
    // device KIDs are globally unique (enforced by DB constraint), so a certificate
    // cannot be replayed for a different device. If a future "rotate device key"
    // feature reuses key material, the message format must be extended (e.g. with
    // account binding or a nonce).
    if verify_ed25519(&root_pubkey_arr, &device_pubkey_bytes, &cert_arr).is_err() {
        return bad_request("Invalid device certificate");
    }

    // Atomic signup: all three inserts in a single transaction.
    // On early return, sqlx auto-rolls back the transaction when `tx` is dropped.
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!("Failed to begin transaction: {e}");
            return internal_error();
        }
    };

    let account =
        match create_account_with_executor(&mut *tx, username, &req.root_pubkey, &root_kid).await {
            Ok(account) => account,
            Err(e) => return account_error_response(e),
        };

    if let Err(e) = create_backup_with_executor(
        &mut *tx,
        account.id,
        &root_kid,
        envelope.as_bytes(),
        envelope.salt(),
        envelope.version(),
    )
    .await
    {
        return backup_error_response(e);
    }

    let device = match create_device_key_with_executor(
        &mut tx,
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

    if let Err(e) = tx.commit().await {
        tracing::error!("Failed to commit signup transaction: {e}");
        return internal_error();
    }

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

fn internal_error() -> axum::response::Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse {
            error: "Internal server error".to_string(),
        }),
    )
        .into_response()
}

pub(super) fn bad_request(msg: &str) -> axum::response::Response {
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
        AccountRepoError::NotFound => {
            // Unreachable from create path — indicates a programming error
            tracing::error!("Unexpected NotFound from account create during signup");
            internal_error()
        }
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
        BackupRepoError::NotFound => {
            // Unreachable from create path — indicates a programming error
            tracing::error!("Unexpected NotFound from backup create during signup");
            internal_error()
        }
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
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ErrorResponse {
                error: "Maximum device limit reached".to_string(),
            }),
        )
            .into_response(),
        DeviceKeyRepoError::NotFound | DeviceKeyRepoError::AlreadyRevoked => {
            // Unreachable from create path — indicates a programming error
            tracing::error!(
                "Unexpected NotFound/AlreadyRevoked from device key create during signup"
            );
            internal_error()
        }
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
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;
    use sqlx::postgres::PgPoolOptions;
    use tc_crypto::{encode_base64url, BackupEnvelope};
    use tower::ServiceExt;

    /// Create a router with a lazy pool that never connects.
    /// Validation-failure tests never reach the DB so this is safe.
    fn test_router_lazy() -> Router {
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgres://fake:fake@localhost/fake")
            .expect("lazy pool");
        Router::new()
            .route("/auth/signup", post(signup))
            .layer(Extension(pool))
    }

    fn test_envelope() -> BackupEnvelope {
        BackupEnvelope::build(
            [0xAA; 16], // salt
            65536,
            3,
            1,           // m_cost, t_cost, p_cost
            [0xBB; 12],  // nonce
            &[0xCC; 48], // ciphertext
        )
        .expect("test envelope")
    }

    /// Helper that holds valid signup fields which can be individually overridden.
    struct SignupBody {
        username: String,
        root_pubkey: String,
        backup_blob: String,
        device_pubkey: String,
        device_name: String,
        certificate: String,
    }

    impl SignupBody {
        fn valid() -> Self {
            let root_signing_key = SigningKey::generate(&mut OsRng);
            let root_pubkey_bytes = root_signing_key.verifying_key().to_bytes();

            let device_signing_key = SigningKey::generate(&mut OsRng);
            let device_pubkey_bytes = device_signing_key.verifying_key().to_bytes();

            let certificate_sig = root_signing_key.sign(&device_pubkey_bytes);

            Self {
                username: "alice".to_string(),
                root_pubkey: encode_base64url(&root_pubkey_bytes),
                backup_blob: encode_base64url(test_envelope().as_bytes()),
                device_pubkey: encode_base64url(&device_pubkey_bytes),
                device_name: "Test Device".to_string(),
                certificate: encode_base64url(&certificate_sig.to_bytes()),
            }
        }

        fn to_json(&self) -> String {
            format!(
                r#"{{"username": "{}", "root_pubkey": "{}", "backup": {{"encrypted_blob": "{}"}}, "device": {{"pubkey": "{}", "name": "{}", "certificate": "{}"}}}}"#,
                self.username,
                self.root_pubkey,
                self.backup_blob,
                self.device_pubkey,
                self.device_name,
                self.certificate
            )
        }
    }

    fn valid_signup_body() -> String {
        SignupBody::valid().to_json()
    }

    fn signup_request(body: &str) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri("/auth/signup")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .expect("request builder")
    }

    // Note: test_signup_success is covered by integration tests in identity_handler_tests.rs
    // since it requires a real database connection for the transaction.

    #[tokio::test]
    async fn test_signup_empty_username() {
        let app = test_router_lazy();

        let body = valid_signup_body().replace("alice", "");
        let response = app.oneshot(signup_request(&body)).await.expect("response");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_signup_invalid_root_pubkey() {
        let app = test_router_lazy();

        let mut sb = SignupBody::valid();
        sb.root_pubkey = "!!!not-base64!!!".to_string();
        let response = app
            .oneshot(signup_request(&sb.to_json()))
            .await
            .expect("response");

        let (parts, body) = response.into_parts();
        assert_eq!(parts.status, StatusCode::BAD_REQUEST);
        let body_bytes = to_bytes(body, 1024 * 1024).await.expect("body bytes");
        let payload: ErrorResponse = serde_json::from_slice(&body_bytes).expect("json payload");
        assert!(payload.error.contains("root_pubkey"));
    }

    #[tokio::test]
    async fn test_signup_short_root_pubkey() {
        let app = test_router_lazy();

        let mut sb = SignupBody::valid();
        sb.root_pubkey = encode_base64url(&[1u8; 4]);
        let response = app
            .oneshot(signup_request(&sb.to_json()))
            .await
            .expect("response");

        let (parts, body) = response.into_parts();
        assert_eq!(parts.status, StatusCode::BAD_REQUEST);
        let body_bytes = to_bytes(body, 1024 * 1024).await.expect("body bytes");
        let payload: ErrorResponse = serde_json::from_slice(&body_bytes).expect("json payload");
        assert!(payload.error.contains("32 bytes"));
    }

    #[tokio::test]
    async fn test_signup_invalid_backup_envelope() {
        let app = test_router_lazy();

        let mut sb = SignupBody::valid();
        sb.backup_blob = encode_base64url(&[0u8; 10]);
        let response = app
            .oneshot(signup_request(&sb.to_json()))
            .await
            .expect("response");

        let (parts, body) = response.into_parts();
        assert_eq!(parts.status, StatusCode::BAD_REQUEST);
        let body_bytes = to_bytes(body, 1024 * 1024).await.expect("body bytes");
        let payload: ErrorResponse = serde_json::from_slice(&body_bytes).expect("json payload");
        assert!(payload.error.contains("envelope"));
    }

    #[tokio::test]
    async fn test_signup_invalid_device_pubkey() {
        let app = test_router_lazy();

        let mut sb = SignupBody::valid();
        sb.device_pubkey = encode_base64url(&[2u8; 16]);
        let response = app
            .oneshot(signup_request(&sb.to_json()))
            .await
            .expect("response");

        let (parts, body) = response.into_parts();
        assert_eq!(parts.status, StatusCode::BAD_REQUEST);
        let body_bytes = to_bytes(body, 1024 * 1024).await.expect("body bytes");
        let payload: ErrorResponse = serde_json::from_slice(&body_bytes).expect("json payload");
        assert!(payload.error.contains("device.pubkey"));
    }

    #[tokio::test]
    async fn test_signup_empty_device_name() {
        let app = test_router_lazy();

        let mut sb = SignupBody::valid();
        sb.device_name = String::new();
        let response = app
            .oneshot(signup_request(&sb.to_json()))
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_signup_invalid_certificate_length() {
        let app = test_router_lazy();

        let mut sb = SignupBody::valid();
        sb.certificate = encode_base64url(&[3u8; 32]);
        let response = app
            .oneshot(signup_request(&sb.to_json()))
            .await
            .expect("response");

        let (parts, body) = response.into_parts();
        assert_eq!(parts.status, StatusCode::BAD_REQUEST);
        let body_bytes = to_bytes(body, 1024 * 1024).await.expect("body bytes");
        let payload: ErrorResponse = serde_json::from_slice(&body_bytes).expect("json payload");
        assert!(payload.error.contains("certificate"));
    }

    #[tokio::test]
    async fn test_signup_invalid_certificate_signature() {
        let app = test_router_lazy();

        let mut sb = SignupBody::valid();
        // Valid length but wrong signature bytes
        sb.certificate = encode_base64url(&[0xFFu8; 64]);
        let response = app
            .oneshot(signup_request(&sb.to_json()))
            .await
            .expect("response");

        let (parts, body) = response.into_parts();
        assert_eq!(parts.status, StatusCode::BAD_REQUEST);
        let body_bytes = to_bytes(body, 1024 * 1024).await.expect("body bytes");
        let payload: ErrorResponse = serde_json::from_slice(&body_bytes).expect("json payload");
        assert!(payload.error.contains("Invalid device certificate"));
    }

    // --- Username validation unit tests (validate_username function) ---
    // These test the validation function directly. The function is wired into
    // the handler and tested via HTTP in the tests above (test_signup_empty_username).

    #[test]
    fn test_validate_username_too_short() {
        assert!(validate_username("ab").is_err());
        assert!(validate_username("a").is_err());
    }

    #[test]
    fn test_validate_username_min_valid_length() {
        assert!(validate_username("abc").is_ok());
    }

    #[test]
    fn test_validate_username_too_long() {
        let long = "a".repeat(65);
        assert!(validate_username(&long).is_err());
    }

    #[test]
    fn test_validate_username_max_valid_length() {
        let max = "a".repeat(64);
        assert!(validate_username(&max).is_ok());
    }

    #[test]
    fn test_validate_username_invalid_chars() {
        let result = validate_username("al!ce");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("letters, numbers, hyphens, and underscores"));
    }

    #[test]
    fn test_validate_username_unicode_rejected() {
        assert!(validate_username("álice").is_err());
    }

    #[test]
    fn test_validate_username_spaces_rejected() {
        assert!(validate_username("al ice").is_err());
    }

    #[test]
    fn test_validate_username_hyphens_underscores_valid() {
        assert!(validate_username("a-b_c").is_ok());
    }

    #[test]
    fn test_validate_username_reserved() {
        let result = validate_username("admin");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("reserved"));
    }

    #[test]
    fn test_validate_username_reserved_case_insensitive() {
        assert!(validate_username("Admin").is_err());
        assert!(validate_username("ROOT").is_err());
    }

    // Note: signup_success, duplicate_username, duplicate_key, and database_error
    // tests are covered by integration tests in identity_handler_tests.rs since they require
    // a real Postgres connection for the transaction-based signup handler.
}
