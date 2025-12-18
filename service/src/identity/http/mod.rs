//! HTTP handlers for identity system

use std::sync::Arc;

use axum::{
    extract::Extension, http::StatusCode, response::IntoResponse, routing::post, Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::repo::{AccountRepo, AccountRepoError};
use tc_crypto::{decode_base64url_native as decode_base64url, derive_kid};

/// Signup request payload
#[derive(Debug, Deserialize)]
pub struct SignupRequest {
    pub username: String,
    pub root_pubkey: String, // base64url encoded
}

/// Signup response
#[derive(Debug, Serialize, Deserialize)]
pub struct SignupResponse {
    pub account_id: Uuid,
    pub root_kid: String,
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

/// Handle signup request
async fn signup(
    Extension(repo): Extension<Arc<dyn AccountRepo>>,
    Json(req): Json<SignupRequest>,
) -> impl IntoResponse {
    // Validate username
    let username = req.username.trim();
    if username.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Username cannot be empty".to_string(),
            }),
        )
            .into_response();
    }

    if username.len() > 64 {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Username too long".to_string(),
            }),
        )
            .into_response();
    }

    // Decode and validate public key
    let Ok(pubkey_bytes) = decode_base64url(&req.root_pubkey) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Invalid base64url encoding for root_pubkey".to_string(),
            }),
        )
            .into_response();
    };

    if pubkey_bytes.len() != 32 {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "root_pubkey must be 32 bytes (Ed25519)".to_string(),
            }),
        )
            .into_response();
    }

    // Derive KID from public key
    let root_kid = derive_kid(&pubkey_bytes);

    // Create account via repository
    match repo.create(username, &req.root_pubkey, &root_kid).await {
        Ok(account) => (
            StatusCode::CREATED,
            Json(SignupResponse {
                account_id: account.id,
                root_kid: account.root_kid,
            }),
        )
            .into_response(),
        Err(e) => match e {
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
                tracing::error!("Signup failed: {}", db_err);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "Internal server error".to_string(),
                    }),
                )
                    .into_response()
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::repo::mock::MockAccountRepo;
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use sqlx::Error as SqlxError;
    use tc_crypto::{derive_kid, encode_base64url};
    use tower::ServiceExt;

    fn test_router(repo: Arc<dyn AccountRepo>) -> Router {
        Router::new()
            .route("/auth/signup", post(signup))
            .layer(Extension(repo))
    }

    fn encoded_pubkey(byte: u8) -> (String, String) {
        let pubkey_bytes = [byte; 32];
        let encoded = encode_base64url(&pubkey_bytes);
        let kid = derive_kid(&pubkey_bytes);
        (encoded, kid)
    }

    #[tokio::test]
    async fn test_signup_success() {
        let mock_repo = Arc::new(MockAccountRepo::new());
        let app = test_router(mock_repo.clone());

        let (root_pubkey, expected_kid) = encoded_pubkey(1);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/signup")
                    .header("content-type", "application/json")
                    .body(Body::from(format!(
                        r#"{{"username": "alice", "root_pubkey": "{root_pubkey}"}}"#
                    )))
                    .expect("request builder"),
            )
            .await
            .expect("response");

        let (parts, body) = response.into_parts();
        assert_eq!(parts.status, StatusCode::CREATED);

        let body_bytes = to_bytes(body, 1024 * 1024).await.expect("body bytes");
        let payload: SignupResponse = serde_json::from_slice(&body_bytes).expect("json payload");

        assert_eq!(payload.root_kid, expected_kid);

        let calls = mock_repo.calls();
        assert_eq!(calls.len(), 1);
        let (username, captured_pubkey, captured_kid) = &calls[0];
        assert_eq!(username, "alice");
        assert_eq!(captured_pubkey, &root_pubkey);
        assert_eq!(captured_kid, &expected_kid);
    }

    #[tokio::test]
    async fn test_signup_empty_username() {
        let mock_repo = Arc::new(MockAccountRepo::new());
        let app = test_router(mock_repo);

        let (root_pubkey, _) = encoded_pubkey(2);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/signup")
                    .header("content-type", "application/json")
                    .body(Body::from(format!(
                        r#"{{"username": "", "root_pubkey": "{root_pubkey}"}}"#
                    )))
                    .expect("request builder"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_signup_invalid_base64_pubkey() {
        let mock_repo = Arc::new(MockAccountRepo::new());
        let app = test_router(mock_repo);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/signup")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"username": "alice", "root_pubkey": "!!!not-base64!!!"}"#,
                    ))
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
            .contains("Invalid base64url encoding for root_pubkey"));
    }

    #[tokio::test]
    async fn test_signup_pubkey_wrong_length() {
        let mock_repo = Arc::new(MockAccountRepo::new());
        let app = test_router(mock_repo);

        // Valid base64 but only encodes 4 bytes.
        let short_pubkey = encode_base64url(&[9u8; 4]);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/signup")
                    .header("content-type", "application/json")
                    .body(Body::from(format!(
                        r#"{{"username": "alice", "root_pubkey": "{short_pubkey}"}}"#
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
            .contains("root_pubkey must be 32 bytes (Ed25519)"));
    }

    #[tokio::test]
    async fn test_signup_duplicate_username() {
        let mock_repo = Arc::new(MockAccountRepo::new());
        mock_repo.set_create_result(Err(AccountRepoError::DuplicateUsername));
        let app = test_router(mock_repo);

        let (root_pubkey, _) = encoded_pubkey(3);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/signup")
                    .header("content-type", "application/json")
                    .body(Body::from(format!(
                        r#"{{"username": "alice", "root_pubkey": "{root_pubkey}"}}"#
                    )))
                    .expect("request builder"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn test_signup_duplicate_key() {
        let mock_repo = Arc::new(MockAccountRepo::new());
        mock_repo.set_create_result(Err(AccountRepoError::DuplicateKey));
        let app = test_router(mock_repo);

        let (root_pubkey, _) = encoded_pubkey(4);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/signup")
                    .header("content-type", "application/json")
                    .body(Body::from(format!(
                        r#"{{"username": "alice", "root_pubkey": "{root_pubkey}"}}"#
                    )))
                    .expect("request builder"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn test_signup_database_error_returns_500() {
        let mock_repo = Arc::new(MockAccountRepo::new());
        mock_repo.set_create_result(Err(AccountRepoError::Database(SqlxError::Io(
            std::io::Error::new(std::io::ErrorKind::Other, "boom"),
        ))));
        let app = test_router(mock_repo);

        let (root_pubkey, _) = encoded_pubkey(5);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/signup")
                    .header("content-type", "application/json")
                    .body(Body::from(format!(
                        r#"{{"username": "alice", "root_pubkey": "{root_pubkey}"}}"#
                    )))
                    .expect("request builder"),
            )
            .await
            .expect("response");

        let (parts, body) = response.into_parts();
        assert_eq!(parts.status, StatusCode::INTERNAL_SERVER_ERROR);

        let body_bytes = to_bytes(body, 1024 * 1024).await.expect("body bytes");
        let payload: ErrorResponse = serde_json::from_slice(&body_bytes).expect("json payload");
        assert!(payload.error.contains("Internal server error"));
    }
}
