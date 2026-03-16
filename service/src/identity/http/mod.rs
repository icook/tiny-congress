//! HTTP handlers for identity system

pub mod auth;
pub mod backup;
pub mod devices;
pub mod login;

use std::sync::Arc;

use axum::{
    extract::{Extension, Query},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::service::{IdentityService, RootPubkey, SignupError, SignupRequest};
use crate::identity::http::auth::AuthenticatedDevice;
use crate::identity::repo::{AccountRecord, AccountRepoError, IdentityRepo};
use tc_crypto::Kid;

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

/// Account lookup response — returns only what the UI needs to target a user.
#[derive(Debug, Serialize)]
pub struct AccountLookupResponse {
    pub id: Uuid,
    pub username: String,
}

/// Query parameters for the account lookup endpoint.
#[derive(Debug, Deserialize)]
pub struct AccountLookupQuery {
    pub username: String,
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
            delete(devices::revoke_device).patch(devices::rename_device),
        )
        .route("/accounts/lookup", get(account_lookup))
}

/// Look up an account by username.
///
/// Returns `{ id, username }` so the caller can use the UUID for trust actions.
/// Requires authentication — callers must have a valid device session.
async fn account_lookup(
    Extension(repo): Extension<Arc<dyn IdentityRepo>>,
    Query(params): Query<AccountLookupQuery>,
    _auth: AuthenticatedDevice,
) -> impl IntoResponse {
    let username = params.username.trim().to_string();
    if username.is_empty() {
        return bad_request("username is required");
    }

    match repo.get_account_by_username(&username).await {
        Ok(account) => (
            StatusCode::OK,
            Json(AccountLookupResponse {
                id: account.id,
                username: account.username,
            }),
        )
            .into_response(),
        Err(AccountRepoError::NotFound) => not_found("user not found"),
        Err(e) => {
            tracing::error!("account_lookup DB error: {e}");
            internal_error()
        }
    }
}

// ── Shared timestamp helpers ─────────────────────────────────────────────────

/// Returns `true` when `timestamp` differs from `now` by more than [`auth::MAX_TIMESTAMP_SKEW`].
///
/// Uses [`i64::abs_diff`] to compute the absolute difference without overflow on
/// extreme values (`i64::MIN`, `i64::MAX`).
pub(crate) const fn timestamp_is_stale(now: i64, timestamp: i64) -> bool {
    now.abs_diff(timestamp) > auth::MAX_TIMESTAMP_SKEW as u64
}

// ── Shared error response helpers ───────────────────────────────────────────

pub(crate) fn bad_request(msg: &str) -> axum::response::Response {
    (
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
            error: msg.to_string(),
        }),
    )
        .into_response()
}

pub(crate) fn not_found(msg: &str) -> axum::response::Response {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorResponse {
            error: msg.to_string(),
        }),
    )
        .into_response()
}

pub(crate) fn unauthorized(msg: &str) -> axum::response::Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(ErrorResponse {
            error: msg.to_string(),
        }),
    )
        .into_response()
}

pub(crate) fn internal_error() -> axum::response::Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse {
            error: "Internal server error".to_string(),
        }),
    )
        .into_response()
}

/// Decode the stored base64url root public key from an account record into raw bytes.
///
/// Both the login and add-device flows look up the root pubkey from the account record
/// to verify a certificate. This helper consolidates the decode-and-check so both
/// callers handle corruption identically.
#[allow(clippy::result_large_err)]
pub(crate) fn decode_account_root_pubkey(
    account: &AccountRecord,
) -> Result<[u8; 32], axum::response::Response> {
    RootPubkey::from_base64url(&account.root_pubkey)
        .map(|k| *k.as_bytes())
        .map_err(|e| {
            tracing::error!("Corrupted root pubkey for account {}: {e}", account.id);
            internal_error()
        })
}

/// Handle signup request — delegates validation and persistence to [`IdentityService`].
async fn signup(
    Extension(service): Extension<Arc<dyn IdentityService>>,
    Json(req): Json<SignupRequest>,
) -> impl IntoResponse {
    match service.signup(&req).await {
        Ok(result) => (
            StatusCode::CREATED,
            Json(SignupResponse {
                account_id: result.account_id,
                root_kid: result.root_kid,
                device_kid: result.device_kid,
            }),
        )
            .into_response(),
        Err(e) => signup_error_response(e),
    }
}

fn signup_error_response(e: SignupError) -> axum::response::Response {
    match e {
        SignupError::Validation(msg) => {
            (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: msg })).into_response()
        }
        SignupError::DuplicateUsername => (
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: "Username already taken".to_string(),
            }),
        )
            .into_response(),
        SignupError::DuplicateKey => (
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: "Public key already registered".to_string(),
            }),
        )
            .into_response(),
        SignupError::MaxDevicesReached => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ErrorResponse {
                error: "Maximum device limit reached".to_string(),
            }),
        )
            .into_response(),
        SignupError::Internal(ref msg) => {
            tracing::error!("Signup returned internal error: {msg}");
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
    use crate::identity::service::mock::MockIdentityService;
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    fn test_router_with_service(service: MockIdentityService) -> Router {
        Router::new()
            .route("/auth/signup", post(signup))
            .layer(Extension(Arc::new(service) as Arc<dyn IdentityService>))
    }

    fn signup_request(body: &str) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri("/auth/signup")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .expect("request builder")
    }

    /// Minimal valid JSON — the mock service doesn't validate, so content doesn't matter
    /// as long as it deserializes into `SignupRequest`.
    fn stub_signup_json() -> String {
        r#"{"username": "x", "root_pubkey": "x", "backup": {"encrypted_blob": "x"}, "device": {"pubkey": "x", "name": "x", "certificate": "x"}}"#.to_string()
    }

    // ── HTTP status code mapping tests ─────────────────────────────────────

    #[tokio::test]
    async fn test_signup_success_returns_created() {
        let svc = MockIdentityService::succeeding();
        let app = test_router_with_service(svc);

        let response = app
            .oneshot(signup_request(&stub_signup_json()))
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::CREATED);
        let body = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body");
        let payload: SignupResponse = serde_json::from_slice(&body).expect("json");
        // verify it has all expected fields
        assert!(!payload.account_id.is_nil());
    }

    #[tokio::test]
    async fn test_signup_validation_error_returns_bad_request() {
        let svc = MockIdentityService::new();
        svc.set_signup_result(Err(SignupError::Validation(
            "Username must be at least 3 characters".to_string(),
        )));
        let app = test_router_with_service(svc);

        let response = app
            .oneshot(signup_request(&stub_signup_json()))
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body");
        let payload: ErrorResponse = serde_json::from_slice(&body).expect("json");
        assert!(payload.error.contains("3 characters"));
    }

    #[tokio::test]
    async fn test_signup_duplicate_username_returns_conflict() {
        let svc = MockIdentityService::new();
        svc.set_signup_result(Err(SignupError::DuplicateUsername));
        let app = test_router_with_service(svc);

        let response = app
            .oneshot(signup_request(&stub_signup_json()))
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body");
        let payload: ErrorResponse = serde_json::from_slice(&body).expect("json");
        assert_eq!(payload.error, "Username already taken");
    }

    #[tokio::test]
    async fn test_signup_duplicate_key_returns_conflict() {
        let svc = MockIdentityService::new();
        svc.set_signup_result(Err(SignupError::DuplicateKey));
        let app = test_router_with_service(svc);

        let response = app
            .oneshot(signup_request(&stub_signup_json()))
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body");
        let payload: ErrorResponse = serde_json::from_slice(&body).expect("json");
        assert_eq!(payload.error, "Public key already registered");
    }

    #[tokio::test]
    async fn test_signup_max_devices_returns_unprocessable() {
        let svc = MockIdentityService::new();
        svc.set_signup_result(Err(SignupError::MaxDevicesReached));
        let app = test_router_with_service(svc);

        let response = app
            .oneshot(signup_request(&stub_signup_json()))
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body");
        let payload: ErrorResponse = serde_json::from_slice(&body).expect("json");
        assert_eq!(payload.error, "Maximum device limit reached");
    }

    #[tokio::test]
    async fn test_signup_internal_error_returns_safe_500() {
        let svc = MockIdentityService::new();
        svc.set_signup_result(Err(SignupError::Internal(
            "secret_password@db-host:5432".to_string(),
        )));
        let app = test_router_with_service(svc);

        let response = app
            .oneshot(signup_request(&stub_signup_json()))
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body");
        let body_str = String::from_utf8(body.to_vec()).expect("utf8");
        assert!(body_str.contains("Internal server error"));
        assert!(!body_str.contains("secret_password"));
        assert!(!body_str.contains("db-host"));
    }

    // ── Helper: decode_account_root_pubkey ─────────────────────────────────

    fn test_account_record(root_pubkey: &str) -> AccountRecord {
        AccountRecord {
            id: Uuid::nil(),
            username: "testuser".to_string(),
            root_pubkey: root_pubkey.to_string(),
            root_kid: Kid::derive(&[0u8; 32]),
        }
    }

    #[test]
    fn test_decode_account_root_pubkey_valid() {
        use tc_crypto::encode_base64url;
        let bytes = [1u8; 32];
        let account = test_account_record(&encode_base64url(&bytes));
        assert_eq!(decode_account_root_pubkey(&account).unwrap(), bytes);
    }

    #[test]
    fn test_decode_account_root_pubkey_invalid_base64() {
        let account = test_account_record("!!!not-base64!!!");
        assert!(decode_account_root_pubkey(&account).is_err());
    }

    #[test]
    fn test_decode_account_root_pubkey_wrong_length() {
        use tc_crypto::encode_base64url;
        let short = encode_base64url(&[1u8; 16]); // 16 bytes, not 32
        let account = test_account_record(&short);
        assert!(decode_account_root_pubkey(&account).is_err());
    }
}
