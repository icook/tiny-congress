//! HTTP handlers for identity system

use std::sync::Arc;

use axum::{
    extract::Extension, http::StatusCode, response::IntoResponse, routing::post, Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::service::{IdentityService, SignupError, SignupRequest};
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

/// Create identity router
pub fn router() -> Router {
    Router::new().route("/auth/signup", post(signup))
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
}
