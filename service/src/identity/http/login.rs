//! Login HTTP handler
//!
//! Authenticates an existing user by verifying a certificate signed by their
//! root key and registers a new device key for the session.
//!
//! ## Replay protection
//!
//! The request includes a `timestamp` (Unix seconds) that must be within
//! ±300 seconds of the server's clock. The certificate signs
//! `device_pubkey || timestamp_le_i64_bytes`, binding the signature to a
//! narrow time window. A SHA-256 hash of the certificate bytes is recorded
//! as a nonce, so replaying the exact same request within the window is
//! rejected.

use std::sync::Arc;

use axum::{extract::Extension, http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::auth::MAX_TIMESTAMP_SKEW;
use super::ErrorResponse;
use crate::identity::repo::{AccountRepoError, DeviceKeyRepoError, IdentityRepo, NonceRepoError};
use crate::identity::service::DeviceName;
use tc_crypto::{decode_base64url, verify_ed25519, Kid};

/// Login request payload
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub timestamp: i64,
    pub device: LoginDevice,
}

/// Device data for login
#[derive(Debug, Deserialize)]
pub struct LoginDevice {
    /// Base64url-encoded Ed25519 public key
    pub pubkey: String,
    /// User-provided device name
    pub name: String,
    /// Base64url-encoded certificate (root key's signature over `device_pubkey || timestamp`)
    pub certificate: String,
}

/// Login response
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub account_id: String,
    pub root_kid: Kid,
    pub device_kid: Kid,
}

/// Validated login fields after input parsing and certificate verification.
struct ValidatedLogin {
    device_kid: Kid,
    device_name: DeviceName,
    cert_bytes: Vec<u8>,
}

/// Validate and verify the login request inputs.
///
/// Checks device pubkey length, device name, certificate format, and verifies
/// the certificate signature over `device_pubkey || timestamp`.
#[allow(clippy::result_large_err)]
fn validate_login_device(
    req: &LoginRequest,
    root_pubkey_arr: &[u8; 32],
) -> Result<ValidatedLogin, axum::response::Response> {
    let Ok(device_pubkey_bytes) = decode_base64url(&req.device.pubkey) else {
        return Err(bad_request("Invalid base64url encoding for device.pubkey"));
    };
    if device_pubkey_bytes.len() != 32 {
        return Err(bad_request("device.pubkey must be 32 bytes (Ed25519)"));
    }

    let device_name =
        DeviceName::parse(&req.device.name).map_err(|e| bad_request(&e.to_string()))?;

    let Ok(cert_bytes) = decode_base64url(&req.device.certificate) else {
        return Err(bad_request(
            "Invalid base64url encoding for device.certificate",
        ));
    };
    let Ok(cert_arr): Result<[u8; 64], _> = cert_bytes.as_slice().try_into() else {
        return Err(bad_request(
            "device.certificate must be 64 bytes (Ed25519 signature)",
        ));
    };

    // The certificate must sign device_pubkey || timestamp (LE i64 bytes)
    let mut signed_payload = Vec::with_capacity(40);
    signed_payload.extend_from_slice(&device_pubkey_bytes);
    signed_payload.extend_from_slice(&req.timestamp.to_le_bytes());

    if verify_ed25519(root_pubkey_arr, &signed_payload, &cert_arr).is_err() {
        return Err(bad_request("Invalid device certificate"));
    }

    let device_kid = Kid::derive(&device_pubkey_bytes);

    Ok(ValidatedLogin {
        device_kid,
        device_name,
        cert_bytes,
    })
}

/// POST /auth/login -- authenticate and register a device key
pub async fn login(
    Extension(repo): Extension<Arc<dyn IdentityRepo>>,
    Json(req): Json<LoginRequest>,
) -> impl IntoResponse {
    // Validate timestamp — use abs_diff to avoid overflow on extreme values
    let now = chrono::Utc::now().timestamp();
    if now.abs_diff(req.timestamp) > MAX_TIMESTAMP_SKEW as u64 {
        return bad_request("Timestamp out of range");
    }

    // Validate username
    let username = req.username.trim();
    if username.is_empty() {
        return bad_request("Username is required");
    }

    // Look up the account by username
    let account = match repo.get_account_by_username(username).await {
        Ok(a) => a,
        Err(AccountRepoError::NotFound) => return bad_request("Invalid credentials"),
        Err(e) => {
            tracing::error!("Login account lookup failed: {e}");
            return internal_error();
        }
    };

    // Decode root public key from the stored account
    let Ok(root_pubkey_bytes) = decode_base64url(&account.root_pubkey) else {
        tracing::error!("Corrupted root pubkey for account {}", account.id);
        return internal_error();
    };
    let Ok(root_pubkey_arr): Result<[u8; 32], _> = root_pubkey_bytes.as_slice().try_into() else {
        tracing::error!("Corrupted root pubkey length for account {}", account.id);
        return internal_error();
    };

    // Validate device fields and verify the timestamp-bound certificate
    let validated = match validate_login_device(&req, &root_pubkey_arr) {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    // Record nonce to prevent replay within the timestamp window
    let nonce_hash: [u8; 32] = Sha256::digest(&validated.cert_bytes).into();
    if let Err(e) = repo.check_and_record_nonce(&nonce_hash).await {
        return match e {
            NonceRepoError::Replay => bad_request("Request replay detected"),
            NonceRepoError::Database(db_err) => {
                tracing::error!("Nonce check failed: {db_err}");
                internal_error()
            }
        };
    }

    // Create device key
    match repo
        .create_device_key(
            account.id,
            &validated.device_kid,
            &req.device.pubkey,
            validated.device_name.as_str(),
            &validated.cert_bytes,
        )
        .await
    {
        Ok(_created) => (
            StatusCode::OK,
            Json(LoginResponse {
                account_id: account.id.to_string(),
                root_kid: account.root_kid,
                device_kid: validated.device_kid,
            }),
        )
            .into_response(),
        Err(DeviceKeyRepoError::DuplicateKid) => (
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: "Device key already registered".to_string(),
            }),
        )
            .into_response(),
        Err(DeviceKeyRepoError::MaxDevicesReached) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ErrorResponse {
                error: "Maximum device limit reached".to_string(),
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Login device creation failed: {e}");
            internal_error()
        }
    }
}

fn bad_request(msg: &str) -> axum::response::Response {
    super::bad_request(msg)
}

fn internal_error() -> axum::response::Response {
    super::internal_error()
}
