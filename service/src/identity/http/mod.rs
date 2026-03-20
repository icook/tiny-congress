// lint-patterns:allow-no-utoipa — tracked by #906
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
use utoipa::ToSchema;
use uuid::Uuid;

use super::service::{validate_username, IdentityService, RootPubkey, SignupError, SignupRequest};
// Re-export shared error helpers so submodules and external callers can use them.
use crate::config::RateLimitConfig;
use crate::http::rate_limit::make_governor_layer;
pub use crate::http::{bad_request, internal_error, not_found, unauthorized, ErrorResponse, Path};
pub(crate) use crate::http::{conflict, forbidden};
use crate::identity::http::auth::AuthenticatedDevice;
use crate::identity::repo::{AccountRecord, AccountRepoError, DeviceKeyRepoError, IdentityRepo};
use tc_crypto::Kid;

/// Signup response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SignupResponse {
    #[schema(value_type = String, format = "uuid")]
    pub account_id: Uuid,
    #[schema(value_type = String)]
    pub root_kid: Kid,
    #[schema(value_type = String)]
    pub device_kid: Kid,
}

/// Account lookup response — returns only what the UI needs to target a user.
#[derive(Debug, Serialize, ToSchema)]
pub struct AccountLookupResponse {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub username: String,
}

/// Query parameters for the account lookup endpoint.
#[derive(Debug, Deserialize)]
pub struct AccountLookupQuery {
    pub username: String,
}

/// Create identity router.
///
/// Unauthenticated endpoints (`/auth/signup`, `/auth/login`,
/// `/auth/backup/{username}`) get individual rate-limit layers based on
/// `rate_limit_config`. Authenticated device-management and lookup routes are
/// not rate-limited here.
pub fn router(rate_limit_config: &RateLimitConfig) -> Router {
    // ── Unauthenticated routes — each gets its own governor layer ──────────
    //
    // Tower layers apply inside-out, so we nest each route in its own
    // single-route Router and apply the corresponding limit there.  Merging
    // three small routers is equivalent to a single router with per-route
    // layers, but avoids sharing one limiter across different routes.

    let signup_router = {
        let r = Router::new().route("/auth/signup", post(signup));
        if let Some(layer) =
            make_governor_layer(rate_limit_config.signup_per_minute, rate_limit_config)
        {
            r.layer(layer)
        } else {
            r
        }
    };

    let login_router = {
        let r = Router::new().route("/auth/login", post(login::login));
        if let Some(layer) =
            make_governor_layer(rate_limit_config.login_per_minute, rate_limit_config)
        {
            r.layer(layer)
        } else {
            r
        }
    };

    let backup_router = {
        let r = Router::new().route("/auth/backup/{username}", get(backup::get_backup));
        if let Some(layer) =
            make_governor_layer(rate_limit_config.backup_per_minute, rate_limit_config)
        {
            r.layer(layer)
        } else {
            r
        }
    };

    // ── Authenticated routes — no rate limiting ────────────────────────────
    let authenticated_router = Router::new()
        .route(
            "/auth/devices",
            get(devices::list_devices).post(devices::add_device),
        )
        .route(
            "/auth/devices/{kid}",
            delete(devices::revoke_device).patch(devices::rename_device),
        )
        .route("/accounts/lookup", get(account_lookup));

    signup_router
        .merge(login_router)
        .merge(backup_router)
        .merge(authenticated_router)
}

/// Look up an account by username.
///
/// Returns `{ id, username }` so the caller can use the UUID for trust actions.
/// Requires authentication — callers must have a valid device session.
#[utoipa::path(
    get,
    path = "/accounts/lookup",
    tag = "Identity",
    params(
        ("username" = String, Query, description = "Username to look up")
    ),
    responses(
        (status = 200, description = "Account found", body = AccountLookupResponse),
        (status = 400, description = "Invalid username"),
        (status = 404, description = "User not found"),
        (status = 500, description = "Internal server error")
    )
)]
async fn account_lookup(
    Extension(repo): Extension<Arc<dyn IdentityRepo>>,
    Query(params): Query<AccountLookupQuery>,
    _auth: AuthenticatedDevice,
) -> impl IntoResponse {
    let username = params.username.trim().to_string();
    if username.is_empty() {
        return bad_request("username is required");
    }
    if let Err(e) = validate_username(&username) {
        return bad_request(&e.to_string());
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

/// Map a [`DeviceKeyRepoError`] to an HTTP response.
///
/// Handles the common mapping used across device management, add-device, and login handlers.
/// Callers that need context-specific messages for [`DeviceKeyRepoError::AlreadyRevoked`]
/// (e.g. "Cannot rename a revoked device") should match on that variant before delegating here.
pub(crate) fn device_key_repo_error_response(e: &DeviceKeyRepoError) -> axum::response::Response {
    match e {
        DeviceKeyRepoError::DuplicateKid => conflict("Device key already registered"),
        DeviceKeyRepoError::MaxDevicesReached => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ErrorResponse {
                error: "Maximum device limit reached".to_string(),
            }),
        )
            .into_response(),
        DeviceKeyRepoError::NotFound => not_found("Device not found"),
        DeviceKeyRepoError::AlreadyRevoked => conflict("Device already revoked"),
        DeviceKeyRepoError::Database(ref db_err) => {
            tracing::error!("Device key repo database error: {db_err}");
            internal_error()
        }
    }
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
#[utoipa::path(
    post,
    path = "/auth/signup",
    tag = "Identity",
    request_body = SignupRequest,
    responses(
        (status = 201, description = "Account created", body = SignupResponse),
        (status = 400, description = "Validation error"),
        (status = 409, description = "Username or key already registered"),
        (status = 422, description = "Maximum device limit reached"),
        (status = 500, description = "Internal server error")
    )
)]
async fn signup(
    Extension(service): Extension<Arc<dyn IdentityService>>,
    Json(req): Json<SignupRequest>,
) -> impl IntoResponse {
    match service.signup(&req).await {
        Ok(result) => {
            tracing::info!(
                username = %req.username,
                account_id = %result.account_id,
                "User signed up"
            );
            (
                StatusCode::CREATED,
                Json(SignupResponse {
                    account_id: result.account_id,
                    root_kid: result.root_kid,
                    device_kid: result.device_kid,
                }),
            )
                .into_response()
        }
        Err(e) => signup_error_response(e),
    }
}

fn signup_error_response(e: SignupError) -> axum::response::Response {
    match e {
        SignupError::Validation(msg) => bad_request(&msg),
        SignupError::DuplicateUsername => conflict("Username already taken"),
        SignupError::DuplicateKey => conflict("Public key already registered"),
        SignupError::MaxDevicesReached => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ErrorResponse {
                error: "Maximum device limit reached".to_string(),
            }),
        )
            .into_response(),
        SignupError::Internal(ref msg) => {
            tracing::error!("Signup returned internal error: {msg}");
            internal_error()
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
