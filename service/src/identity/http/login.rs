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
use uuid::Uuid;

use super::auth::MAX_TIMESTAMP_SKEW;
use super::ErrorResponse;
use crate::identity::repo::{AccountRepoError, DeviceKeyRepoError, IdentityRepo, NonceRepoError};
use crate::identity::service::{CertificateSignature, DeviceName, DevicePubkey};
use tc_crypto::{verify_ed25519, Kid};

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
    pub account_id: Uuid,
    pub root_kid: Kid,
    pub device_kid: Kid,
}

/// Validated login fields after input parsing and certificate verification.
struct ValidatedLogin {
    device_kid: Kid,
    device_name: DeviceName,
    cert: CertificateSignature,
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
    let device_pubkey = DevicePubkey::from_base64url(&req.device.pubkey)
        .map_err(|e| super::bad_request(&e.to_string()))?;

    let device_name =
        DeviceName::parse(&req.device.name).map_err(|e| super::bad_request(&e.to_string()))?;

    let cert_sig = CertificateSignature::from_base64url(&req.device.certificate)
        .map_err(|e| super::bad_request(&e.to_string()))?;

    // The certificate must sign device_pubkey || timestamp (LE i64 bytes)
    let mut signed_payload = Vec::with_capacity(40);
    signed_payload.extend_from_slice(device_pubkey.as_bytes());
    signed_payload.extend_from_slice(&req.timestamp.to_le_bytes());

    if verify_ed25519(root_pubkey_arr, &signed_payload, cert_sig.as_bytes()).is_err() {
        // Return 401 with generic message — must be indistinguishable from
        // AccountNotFound to prevent username enumeration.
        return Err(super::unauthorized("Invalid credentials"));
    }

    let device_kid = device_pubkey.kid();

    Ok(ValidatedLogin {
        device_kid,
        device_name,
        cert: cert_sig,
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
        return super::bad_request("Timestamp out of range");
    }

    // Validate username
    let username = req.username.trim();
    if username.is_empty() {
        return super::bad_request("Username is required");
    }
    if username.len() > crate::identity::service::MAX_USERNAME_LEN {
        return super::bad_request("Username too long");
    }

    // Look up the account by username
    let account = match repo.get_account_by_username(username).await {
        Ok(a) => a,
        // Return 401 with generic message — indistinguishable from
        // InvalidCertificate to prevent username enumeration.
        Err(AccountRepoError::NotFound) => return super::unauthorized("Invalid credentials"),
        Err(e) => {
            tracing::error!("Login account lookup failed: {e}");
            return super::internal_error();
        }
    };

    // Decode root public key from the stored account
    let root_pubkey_arr = match super::decode_account_root_pubkey(&account) {
        Ok(arr) => arr,
        Err(resp) => return resp,
    };

    // Validate device fields and verify the timestamp-bound certificate
    let validated = match validate_login_device(&req, &root_pubkey_arr) {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    // Record nonce to prevent replay within the timestamp window.
    // Nonce cleanup is handled by the background sweep in main.rs
    // (spawn_nonce_cleanup), using MAX_TIMESTAMP_SKEW as the TTL.
    let nonce_hash: [u8; 32] = Sha256::digest(validated.cert.as_bytes()).into();
    if let Err(e) = repo.check_and_record_nonce(&nonce_hash).await {
        return match e {
            NonceRepoError::Replay => super::bad_request("Request replay detected"),
            NonceRepoError::Database(db_err) => {
                tracing::error!("Nonce check failed: {db_err}");
                super::internal_error()
            }
        };
    }

    // Nonce is intentionally recorded before create_device_key: if device
    // creation fails transiently, the user must generate a fresh
    // timestamp-bound certificate rather than retry. This is fail-closed.

    // Create device key
    match repo
        .create_device_key(
            account.id,
            &validated.device_kid,
            &req.device.pubkey,
            validated.device_name.as_str(),
            validated.cert.as_bytes(),
        )
        .await
    {
        Ok(_created) => (
            StatusCode::CREATED,
            Json(LoginResponse {
                account_id: account.id,
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
        Err(e) => {
            tracing::error!("Login device creation failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Internal error — please retry with a new certificate".to_string(),
                }),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::repo::mock::MockIdentityRepo;
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
        routing::post,
        Router,
    };
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;
    use std::sync::Arc;
    use tc_crypto::encode_base64url;
    use tower::ServiceExt;

    /// Build a valid login request and matching root pubkey.
    ///
    /// The certificate signs `device_pubkey || timestamp` with the root key,
    /// matching the format expected by `validate_login_device`.
    fn make_valid_components() -> (LoginRequest, [u8; 32]) {
        let root_key = SigningKey::generate(&mut OsRng);
        let root_pubkey = root_key.verifying_key().to_bytes();

        let device_key = SigningKey::generate(&mut OsRng);
        let device_pubkey = device_key.verifying_key().to_bytes();

        let timestamp = chrono::Utc::now().timestamp();

        let mut msg = Vec::with_capacity(40);
        msg.extend_from_slice(&device_pubkey);
        msg.extend_from_slice(&timestamp.to_le_bytes());
        let sig = root_key.sign(&msg);

        let req = LoginRequest {
            username: "alice".to_string(),
            timestamp,
            device: LoginDevice {
                pubkey: encode_base64url(&device_pubkey),
                name: "My Device".to_string(),
                certificate: encode_base64url(&sig.to_bytes()),
            },
        };

        (req, root_pubkey)
    }

    #[test]
    fn test_validate_login_device_valid() {
        let (req, root_pubkey) = make_valid_components();
        let result = validate_login_device(&req, &root_pubkey);
        assert!(result.is_ok());
        let validated = result.unwrap();
        let expected_kid = DevicePubkey::from_base64url(&req.device.pubkey)
            .unwrap()
            .kid();
        assert_eq!(validated.device_kid, expected_kid);
        assert_eq!(validated.device_name.as_str(), "My Device");
    }

    #[test]
    fn test_validate_login_device_invalid_pubkey_encoding() {
        let (mut req, root_pubkey) = make_valid_components();
        req.device.pubkey = "!!!not-base64!!!".to_string();
        let err = validate_login_device(&req, &root_pubkey)
            .err()
            .expect("expected validation error");
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_validate_login_device_invalid_pubkey_length() {
        let (mut req, root_pubkey) = make_valid_components();
        req.device.pubkey = encode_base64url(&[1u8; 16]); // 16 bytes, not 32
        let err = validate_login_device(&req, &root_pubkey)
            .err()
            .expect("expected validation error");
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_validate_login_device_empty_name() {
        let (mut req, root_pubkey) = make_valid_components();
        req.device.name = String::new();
        let err = validate_login_device(&req, &root_pubkey)
            .err()
            .expect("expected validation error");
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_validate_login_device_invalid_cert_length() {
        let (mut req, root_pubkey) = make_valid_components();
        req.device.certificate = encode_base64url(&[0u8; 32]); // 32 bytes, not 64
        let err = validate_login_device(&req, &root_pubkey)
            .err()
            .expect("expected validation error");
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_validate_login_device_wrong_signature() {
        let (mut req, root_pubkey) = make_valid_components();
        req.device.certificate = encode_base64url(&[0xFFu8; 64]); // bytes don't form a valid sig
        let err = validate_login_device(&req, &root_pubkey)
            .err()
            .expect("expected validation error");
        assert_eq!(err.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_validate_login_device_cert_signed_with_wrong_timestamp() {
        // Certificate is valid but was signed for a different timestamp —
        // the signature covers device_pubkey || timestamp, so the timestamp
        // mismatch must cause verification to fail.
        let root_key = SigningKey::generate(&mut OsRng);
        let root_pubkey = root_key.verifying_key().to_bytes();

        let device_key = SigningKey::generate(&mut OsRng);
        let device_pubkey = device_key.verifying_key().to_bytes();

        let real_timestamp = chrono::Utc::now().timestamp();
        let signed_timestamp = real_timestamp - 100; // what the sig actually covers

        let mut msg = Vec::with_capacity(40);
        msg.extend_from_slice(&device_pubkey);
        msg.extend_from_slice(&signed_timestamp.to_le_bytes());
        let sig = root_key.sign(&msg);

        let req = LoginRequest {
            username: "alice".to_string(),
            timestamp: real_timestamp, // differs from what was signed
            device: LoginDevice {
                pubkey: encode_base64url(&device_pubkey),
                name: "My Device".to_string(),
                certificate: encode_base64url(&sig.to_bytes()),
            },
        };

        let err = validate_login_device(&req, &root_pubkey)
            .err()
            .expect("expected validation error");
        assert_eq!(err.status(), StatusCode::UNAUTHORIZED);
    }

    // ── Handler-level tests ─────────────────────────────────────────────────

    fn test_login_router(repo: MockIdentityRepo) -> Router {
        Router::new()
            .route("/auth/login", post(login))
            .layer(axum::extract::Extension(
                Arc::new(repo) as Arc<dyn crate::identity::repo::IdentityRepo>
            ))
    }

    #[tokio::test]
    async fn test_login_too_long_username_returns_bad_request() {
        let repo = MockIdentityRepo::new();
        let app = test_login_router(repo);

        let long_username = "a".repeat(65);
        let body = serde_json::json!({
            "username": long_username,
            "timestamp": chrono::Utc::now().timestamp(),
            "device": {
                "pubkey": encode_base64url(&[0u8; 32]),
                "name": "test",
                "certificate": encode_base64url(&[0u8; 64])
            }
        })
        .to_string();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/login")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .expect("request builder"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body_bytes = to_bytes(response.into_body(), 1024).await.expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&body_bytes).expect("json");
        assert!(payload["error"].as_str().unwrap().contains("too long"));
    }
}
