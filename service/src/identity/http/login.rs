//! Login endpoint -- authorize a new device using root-key-signed certificate.

use axum::{extract::Extension, http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use super::ErrorResponse;
use crate::identity::repo::{
    create_device_key_with_executor, get_account_by_username, AccountRepoError, DeviceKeyRepoError,
};
use tc_crypto::{decode_base64url_native as decode_base64url, verify_ed25519, Kid};

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub device: LoginDevice,
}

#[derive(Debug, Deserialize)]
pub struct LoginDevice {
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
    Extension(pool): Extension<PgPool>,
    Json(req): Json<LoginRequest>,
) -> impl IntoResponse {
    let username = req.username.trim();
    if username.is_empty() {
        return super::bad_request("Username cannot be empty");
    }

    // Validate device pubkey
    let Ok(device_pubkey_bytes) = decode_base64url(&req.device.pubkey) else {
        return super::bad_request("Invalid base64url encoding for pubkey");
    };
    if device_pubkey_bytes.len() != 32 {
        return super::bad_request("pubkey must be 32 bytes (Ed25519)");
    }

    // Validate device name
    let device_name = req.device.name.trim();
    if device_name.is_empty() {
        return super::bad_request("Device name cannot be empty");
    }
    if device_name.chars().count() > 128 {
        return super::bad_request("Device name too long");
    }

    // Validate certificate
    let Ok(cert_bytes) = decode_base64url(&req.device.certificate) else {
        return super::bad_request("Invalid base64url encoding for certificate");
    };
    let Ok(cert_arr): Result<[u8; 64], _> = cert_bytes.as_slice().try_into() else {
        return super::bad_request("certificate must be 64 bytes (Ed25519 signature)");
    };

    // Look up account
    let account = match get_account_by_username(&pool, username).await {
        Ok(a) => a,
        Err(AccountRepoError::NotFound) => return not_found("Account not found"),
        Err(e) => {
            tracing::error!("Failed to look up account: {e}");
            return super::internal_error();
        }
    };

    // Verify certificate against root pubkey
    let Ok(root_pubkey_bytes) = decode_base64url(&account.root_pubkey) else {
        tracing::error!("Corrupted root pubkey for account {}", account.id);
        return super::internal_error();
    };
    let Ok(root_pubkey_arr): Result<[u8; 32], _> = root_pubkey_bytes.as_slice().try_into() else {
        tracing::error!("Corrupted root pubkey length for account {}", account.id);
        return super::internal_error();
    };

    if verify_ed25519(&root_pubkey_arr, &device_pubkey_bytes, &cert_arr).is_err() {
        return super::bad_request("Invalid device certificate");
    }

    let device_kid = Kid::derive(&device_pubkey_bytes);

    // Create device key in a transaction
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!("Failed to begin transaction: {e}");
            return super::internal_error();
        }
    };

    let created = match create_device_key_with_executor(
        &mut tx,
        account.id,
        &device_kid,
        &req.device.pubkey,
        device_name,
        &cert_bytes,
    )
    .await
    {
        Ok(c) => c,
        Err(DeviceKeyRepoError::DuplicateKid) => {
            return (
                StatusCode::CONFLICT,
                Json(ErrorResponse {
                    error: "Device key already registered".to_string(),
                }),
            )
                .into_response();
        }
        Err(DeviceKeyRepoError::MaxDevicesReached) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(ErrorResponse {
                    error: "Maximum device limit reached".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("Failed to create device key: {e}");
            return super::internal_error();
        }
    };

    if let Err(e) = tx.commit().await {
        tracing::error!("Failed to commit login transaction: {e}");
        return super::internal_error();
    }

    (
        StatusCode::CREATED,
        Json(LoginResponse {
            account_id: account.id,
            root_kid: account.root_kid,
            device_kid: created.device_kid,
        }),
    )
        .into_response()
}

fn not_found(msg: &str) -> axum::response::Response {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorResponse {
            error: msg.to_string(),
        }),
    )
        .into_response()
}
