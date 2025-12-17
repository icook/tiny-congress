//! HTTP handlers for identity system

use axum::{
    extract::Extension, http::StatusCode, response::IntoResponse, routing::post, Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use super::crypto::{decode_base64url, derive_kid};

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
    Extension(pool): Extension<PgPool>,
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

    // Insert account
    let account_id = Uuid::new_v4();
    let result = sqlx::query(
        r"
        INSERT INTO accounts (id, username, root_pubkey, root_kid)
        VALUES ($1, $2, $3, $4)
        ",
    )
    .bind(account_id)
    .bind(username)
    .bind(&req.root_pubkey)
    .bind(&root_kid)
    .execute(&pool)
    .await;

    match result {
        Ok(_) => (
            StatusCode::CREATED,
            Json(SignupResponse {
                account_id,
                root_kid,
            }),
        )
            .into_response(),
        Err(e) => {
            // Check for unique constraint violation using structured error info
            if let sqlx::Error::Database(db_err) = &e {
                if let Some(constraint) = db_err.constraint() {
                    let error_msg = match constraint {
                        "accounts_username_key" => Some("Username already taken"),
                        "accounts_root_kid_key" => Some("Public key already registered"),
                        _ => None,
                    };
                    if let Some(msg) = error_msg {
                        return (
                            StatusCode::CONFLICT,
                            Json(ErrorResponse {
                                error: msg.to_string(),
                            }),
                        )
                            .into_response();
                    }
                }
            }

            tracing::error!("Signup failed: {}", e);
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
