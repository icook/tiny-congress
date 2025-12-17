//! HTTP handlers for identity system

use std::sync::Arc;

use axum::{
    extract::Extension, http::StatusCode, response::IntoResponse, routing::post, Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::crypto::{decode_base64url, derive_kid};
use super::repo::{AccountRepo, AccountRepoError};

/// Signup request payload
#[derive(Debug, Deserialize)]
pub struct SignupRequest {
    pub username: String,
    pub root_pubkey: String, // base64url encoded
}

/// Signup response
#[derive(Debug, Serialize)]
pub struct SignupResponse {
    pub account_id: Uuid,
    pub root_kid: String,
}

/// Error response
#[derive(Debug, Serialize)]
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
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    fn test_router(repo: Arc<dyn AccountRepo>) -> Router {
        Router::new()
            .route("/auth/signup", post(signup))
            .layer(Extension(repo))
    }

    #[tokio::test]
    async fn test_signup_success() {
        let mock_repo = Arc::new(MockAccountRepo::new());
        let app = test_router(mock_repo);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/signup")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"username": "alice", "root_pubkey": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}"#,
                    ))
                    .expect("request builder"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn test_signup_empty_username() {
        let mock_repo = Arc::new(MockAccountRepo::new());
        let app = test_router(mock_repo);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/signup")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"username": "", "root_pubkey": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}"#,
                    ))
                    .expect("request builder"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_signup_duplicate_username() {
        let mock_repo = Arc::new(MockAccountRepo::new());
        mock_repo.set_create_result(Err(AccountRepoError::DuplicateUsername));
        let app = test_router(mock_repo);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/signup")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"username": "alice", "root_pubkey": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}"#,
                    ))
                    .expect("request builder"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::CONFLICT);
    }
}
