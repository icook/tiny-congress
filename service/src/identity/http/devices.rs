//! Device management HTTP handlers
//!
//! Endpoints for listing, adding, revoking, and renaming device keys.
//! All endpoints require authentication via signed headers.

use std::sync::Arc;

use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::auth::AuthenticatedDevice;
use super::ErrorResponse;
use crate::identity::repo::{AccountRepoError, DeviceKeyRecord, DeviceKeyRepoError, IdentityRepo};
use tc_crypto::{decode_base64url, verify_ed25519, Kid};

/// Device info returned in API responses (omits certificate and raw pubkey)
#[derive(Debug, Serialize)]
pub struct DeviceInfo {
    pub device_kid: Kid,
    pub device_name: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

impl From<DeviceKeyRecord> for DeviceInfo {
    fn from(record: DeviceKeyRecord) -> Self {
        Self {
            device_kid: record.device_kid,
            device_name: record.device_name,
            created_at: record.created_at,
            last_used_at: record.last_used_at,
            revoked_at: record.revoked_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DeviceListResponse {
    pub devices: Vec<DeviceInfo>,
}

#[derive(Debug, Deserialize)]
pub struct AddDeviceRequest {
    pub pubkey: String,
    pub name: String,
    pub certificate: String,
}

#[derive(Debug, Serialize)]
pub struct AddDeviceResponse {
    pub device_kid: Kid,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct RenameDeviceRequest {
    pub name: String,
}

/// GET /auth/devices — list all devices for the authenticated account
pub async fn list_devices(
    Extension(repo): Extension<Arc<dyn IdentityRepo>>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    match repo.list_device_keys_by_account(auth.account_id).await {
        Ok(records) => {
            let devices: Vec<DeviceInfo> = records.into_iter().map(DeviceInfo::from).collect();
            (StatusCode::OK, Json(DeviceListResponse { devices })).into_response()
        }
        Err(e) => {
            tracing::error!("Failed to list devices: {e}");
            internal_error()
        }
    }
}

/// POST /auth/devices — add a new device key
pub async fn add_device(
    Extension(repo): Extension<Arc<dyn IdentityRepo>>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    let req: AddDeviceRequest = match auth.json() {
        Ok(r) => r,
        Err(resp) => return resp,
    };

    let validated = match validate_add_device_request(&*repo, auth.account_id, &req).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    match repo
        .create_device_key(
            auth.account_id,
            &validated.device_kid,
            &req.pubkey,
            &validated.device_name,
            &validated.cert_bytes,
        )
        .await
    {
        Ok(created) => (
            StatusCode::CREATED,
            Json(AddDeviceResponse {
                device_kid: created.device_kid,
                created_at: created.created_at,
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
            internal_error()
        }
    }
}

/// Validated fields for adding a device, after input validation and certificate check.
struct ValidatedAddDevice {
    device_kid: Kid,
    device_name: String,
    cert_bytes: Vec<u8>,
}

/// Validate and verify the add-device request inputs.
#[allow(clippy::result_large_err)]
async fn validate_add_device_request(
    repo: &dyn IdentityRepo,
    account_id: Uuid,
    req: &AddDeviceRequest,
) -> Result<ValidatedAddDevice, axum::response::Response> {
    let Ok(device_pubkey_bytes) = decode_base64url(&req.pubkey) else {
        return Err(bad_request("Invalid base64url encoding for pubkey"));
    };
    if device_pubkey_bytes.len() != 32 {
        return Err(bad_request("pubkey must be 32 bytes (Ed25519)"));
    }

    let device_name = req.name.trim();
    if device_name.is_empty() {
        return Err(bad_request("Device name cannot be empty"));
    }
    if device_name.chars().count() > 128 {
        return Err(bad_request("Device name too long"));
    }

    let Ok(cert_bytes) = decode_base64url(&req.certificate) else {
        return Err(bad_request("Invalid base64url encoding for certificate"));
    };
    let Ok(cert_arr): Result<[u8; 64], _> = cert_bytes.as_slice().try_into() else {
        return Err(bad_request(
            "certificate must be 64 bytes (Ed25519 signature)",
        ));
    };

    // Look up the account to get the root pubkey for certificate verification
    let account = match repo.get_account_by_id(account_id).await {
        Ok(a) => a,
        Err(AccountRepoError::NotFound) => {
            tracing::error!("Authenticated device's account not found: {account_id}");
            return Err(internal_error());
        }
        Err(e) => {
            tracing::error!("Failed to look up account: {e}");
            return Err(internal_error());
        }
    };

    let Ok(root_pubkey_bytes) = decode_base64url(&account.root_pubkey) else {
        tracing::error!("Corrupted root pubkey for account {account_id}");
        return Err(internal_error());
    };
    let Ok(root_pubkey_arr): Result<[u8; 32], _> = root_pubkey_bytes.as_slice().try_into() else {
        tracing::error!("Corrupted root pubkey length for account {account_id}");
        return Err(internal_error());
    };

    if verify_ed25519(&root_pubkey_arr, &device_pubkey_bytes, &cert_arr).is_err() {
        return Err(bad_request("Invalid device certificate"));
    }

    let device_kid = Kid::derive(&device_pubkey_bytes);

    Ok(ValidatedAddDevice {
        device_kid,
        device_name: device_name.to_string(),
        cert_bytes,
    })
}

/// DELETE /auth/devices/:kid — revoke a device key
pub async fn revoke_device(
    Extension(repo): Extension<Arc<dyn IdentityRepo>>,
    Path(kid_str): Path<String>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    let kid: Kid = match kid_str.parse() {
        Ok(k) => k,
        Err(_) => return bad_request("Invalid KID format"),
    };

    // Prevent revoking the currently authenticated device
    if kid == auth.device_kid {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ErrorResponse {
                error: "Cannot revoke the device making this request".to_string(),
            }),
        )
            .into_response();
    }

    // Verify the target device belongs to this account
    match get_owned_device(&*repo, &kid, auth.account_id).await {
        Ok(_) => {}
        Err(resp) => return resp,
    }

    match repo.revoke_device_key(&kid).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(DeviceKeyRepoError::AlreadyRevoked) => (
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: "Device already revoked".to_string(),
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to revoke device: {e}");
            internal_error()
        }
    }
}

/// PATCH /auth/devices/:kid — rename a device
pub async fn rename_device(
    Extension(repo): Extension<Arc<dyn IdentityRepo>>,
    Path(kid_str): Path<String>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    let req: RenameDeviceRequest = match auth.json() {
        Ok(r) => r,
        Err(resp) => return resp,
    };

    let kid: Kid = match kid_str.parse() {
        Ok(k) => k,
        Err(_) => return bad_request("Invalid KID format"),
    };

    let new_name = req.name.trim();
    if new_name.is_empty() {
        return bad_request("Device name cannot be empty");
    }
    if new_name.chars().count() > 128 {
        return bad_request("Device name too long");
    }

    match get_owned_device(&*repo, &kid, auth.account_id).await {
        Ok(_) => {}
        Err(resp) => return resp,
    }

    match repo.rename_device_key(&kid, new_name).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(DeviceKeyRepoError::AlreadyRevoked) => (
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: "Cannot rename a revoked device".to_string(),
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to rename device: {e}");
            internal_error()
        }
    }
}

/// Verify a device exists and belongs to the given account.
#[allow(clippy::result_large_err)]
async fn get_owned_device(
    repo: &dyn IdentityRepo,
    kid: &Kid,
    account_id: Uuid,
) -> Result<DeviceKeyRecord, axum::response::Response> {
    let device = match repo.get_device_key_by_kid(kid).await {
        Ok(d) => d,
        Err(DeviceKeyRepoError::NotFound) => {
            return Err(not_found("Device not found"));
        }
        Err(e) => {
            tracing::error!("Failed to look up device: {e}");
            return Err(internal_error());
        }
    };

    if device.account_id != account_id {
        return Err(not_found("Device not found"));
    }

    Ok(device)
}

fn bad_request(msg: &str) -> axum::response::Response {
    super::bad_request(msg)
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

fn internal_error() -> axum::response::Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse {
            error: "Internal server error".to_string(),
        }),
    )
        .into_response()
}
