//! Login endpoint -- authorize a new device using root-key-signed certificate.
//!
//! Delegates all validation and persistence to [`IdentityService::login`].

use std::sync::Arc;

use axum::{extract::Extension, http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::ErrorResponse;
use crate::identity::service::{IdentityService, LoginError, LoginRequest};
use tc_crypto::Kid;

#[derive(Debug, Deserialize)]
pub struct LoginHttpRequest {
    pub username: String,
    pub device: LoginHttpDevice,
}

#[derive(Debug, Deserialize)]
pub struct LoginHttpDevice {
    pub pubkey: String,
    pub name: String,
    pub certificate: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub account_id: Uuid,
    pub root_kid: Kid,
    pub device_kid: Kid,
}

/// POST /auth/login -- authorize new device via root key certificate
pub async fn login(
    Extension(service): Extension<Arc<dyn IdentityService>>,
    Json(req): Json<LoginHttpRequest>,
) -> impl IntoResponse {
    let login_req = LoginRequest {
        username: req.username,
        device_pubkey: req.device.pubkey,
        device_name: req.device.name,
        certificate: req.device.certificate,
    };

    match service.login(login_req).await {
        Ok(result) => (
            StatusCode::CREATED,
            Json(LoginResponse {
                account_id: result.account_id,
                root_kid: result.root_kid,
                device_kid: result.device_kid,
            }),
        )
            .into_response(),
        Err(e) => map_login_error(&e),
    }
}

fn map_login_error(e: &LoginError) -> axum::response::Response {
    match e {
        LoginError::EmptyUsername
        | LoginError::InvalidDeviceName(_)
        | LoginError::InvalidPubkeyEncoding
        | LoginError::InvalidPubkeyLength
        | LoginError::InvalidCertEncoding
        | LoginError::InvalidCertLength
        | LoginError::InvalidCertificate => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
        LoginError::AccountNotFound => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Account not found".to_string(),
            }),
        )
            .into_response(),
        LoginError::DuplicateDevice => (
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: "Device key already registered".to_string(),
            }),
        )
            .into_response(),
        LoginError::MaxDevicesReached => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ErrorResponse {
                error: "Maximum device limit reached".to_string(),
            }),
        )
            .into_response(),
        LoginError::Internal(msg) => {
            tracing::error!("Login returned internal error: {msg}");
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
