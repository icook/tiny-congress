//! Login endpoint -- authorize a new device using root-key-signed certificate.

use std::sync::Arc;

use axum::{extract::Extension, http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::ErrorResponse;
use crate::identity::repo::{AccountRepoError, DeviceKeyRepoError, IdentityRepo};
use crate::identity::service::DeviceName;
use tc_crypto::{decode_base64url, verify_ed25519, Kid};

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
    Extension(repo): Extension<Arc<dyn IdentityRepo>>,
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
    let device_name = match DeviceName::parse(&req.device.name) {
        Ok(n) => n,
        Err(e) => return super::bad_request(&e.to_string()),
    };

    // Validate certificate
    let Ok(cert_bytes) = decode_base64url(&req.device.certificate) else {
        return super::bad_request("Invalid base64url encoding for certificate");
    };
    let Ok(cert_arr): Result<[u8; 64], _> = cert_bytes.as_slice().try_into() else {
        return super::bad_request("certificate must be 64 bytes (Ed25519 signature)");
    };

    // Look up account
    let account = match repo.get_account_by_username(username).await {
        Ok(a) => a,
        Err(AccountRepoError::NotFound) => return super::not_found("Account not found"),
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

    // Create device key via repo trait
    match repo
        .create_device_key(
            account.id,
            &device_kid,
            &req.device.pubkey,
            device_name.as_str(),
            &cert_bytes,
        )
        .await
    {
        Ok(created) => (
            StatusCode::CREATED,
            Json(LoginResponse {
                account_id: account.id,
                root_kid: account.root_kid,
                device_kid: created.device_kid,
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
            tracing::error!("Failed to create device key: {e}");
            super::internal_error()
        }
    }
}
