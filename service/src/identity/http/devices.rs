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
use crate::identity::service::{CertificateSignature, DeviceName, DevicePubkey};
use tc_crypto::{verify_ed25519, Kid};

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
            super::internal_error()
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
            validated.device_name.as_str(),
            validated.cert.as_bytes(),
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
            super::internal_error()
        }
    }
}

/// Validated fields for adding a device, after input validation and certificate check.
struct ValidatedAddDevice {
    device_kid: Kid,
    device_name: DeviceName,
    cert: CertificateSignature,
}

/// Validate and verify the add-device request inputs.
#[allow(clippy::result_large_err)]
async fn validate_add_device_request(
    repo: &dyn IdentityRepo,
    account_id: Uuid,
    req: &AddDeviceRequest,
) -> Result<ValidatedAddDevice, axum::response::Response> {
    let device_pubkey = DevicePubkey::from_base64url(&req.pubkey)
        .map_err(|e| super::bad_request(&e.to_string()))?;

    let device_name =
        DeviceName::parse(&req.name).map_err(|e| super::bad_request(&e.to_string()))?;

    let cert_sig = CertificateSignature::from_base64url(&req.certificate)
        .map_err(|e| super::bad_request(&e.to_string()))?;

    // Look up the account to get the root pubkey for certificate verification
    let account = match repo.get_account_by_id(account_id).await {
        Ok(a) => a,
        Err(AccountRepoError::NotFound) => {
            tracing::error!("Authenticated device's account not found: {account_id}");
            return Err(super::internal_error());
        }
        Err(e) => {
            tracing::error!("Failed to look up account: {e}");
            return Err(super::internal_error());
        }
    };

    let root_pubkey_arr = super::decode_account_root_pubkey(&account)?;

    if verify_ed25519(
        &root_pubkey_arr,
        device_pubkey.as_bytes(),
        cert_sig.as_bytes(),
    )
    .is_err()
    {
        return Err(super::bad_request("Invalid device certificate"));
    }

    let device_kid = device_pubkey.kid();

    Ok(ValidatedAddDevice {
        device_kid,
        device_name,
        cert: cert_sig,
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
        Err(_) => return super::bad_request("Invalid KID format"),
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
            super::internal_error()
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
        Err(_) => return super::bad_request("Invalid KID format"),
    };

    let new_name = match DeviceName::parse(&req.name) {
        Ok(n) => n,
        Err(e) => return super::bad_request(&e.to_string()),
    };

    match get_owned_device(&*repo, &kid, auth.account_id).await {
        Ok(_) => {}
        Err(resp) => return resp,
    }

    match repo.rename_device_key(&kid, new_name.as_str()).await {
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
            super::internal_error()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::repo::mock::MockIdentityRepo;
    use crate::identity::repo::AccountRecord;
    use axum::http::StatusCode;
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;
    use tc_crypto::{encode_base64url, Kid};

    /// Build a valid add-device request and a matching account record.
    ///
    /// The certificate signs the raw 32-byte device pubkey with the root key,
    /// matching the format expected by `validate_add_device_request`.
    fn make_valid_components() -> (AddDeviceRequest, AccountRecord) {
        let root_key = SigningKey::generate(&mut OsRng);
        let root_pubkey = root_key.verifying_key().to_bytes();

        let device_key = SigningKey::generate(&mut OsRng);
        let device_pubkey = device_key.verifying_key().to_bytes();

        let sig = root_key.sign(&device_pubkey);

        let account = AccountRecord {
            id: Uuid::new_v4(),
            username: "alice".to_string(),
            root_pubkey: encode_base64url(&root_pubkey),
            root_kid: Kid::derive(&root_pubkey),
        };

        let req = AddDeviceRequest {
            pubkey: encode_base64url(&device_pubkey),
            name: "New Device".to_string(),
            certificate: encode_base64url(&sig.to_bytes()),
        };

        (req, account)
    }

    fn mock_with_account(account: AccountRecord) -> MockIdentityRepo {
        let repo = MockIdentityRepo::new();
        repo.set_account_by_id_result(Ok(account));
        repo
    }

    #[tokio::test]
    async fn test_validate_add_device_request_valid() {
        let (req, account) = make_valid_components();
        let expected_kid = DevicePubkey::from_base64url(&req.pubkey).unwrap().kid();
        let repo = mock_with_account(account.clone());
        let result = validate_add_device_request(&repo, account.id, &req).await;
        assert!(result.is_ok());
        let validated = result.unwrap();
        assert_eq!(validated.device_kid, expected_kid);
        assert_eq!(validated.device_name.as_str(), "New Device");
    }

    #[tokio::test]
    async fn test_validate_add_device_request_invalid_pubkey_encoding() {
        let (mut req, account) = make_valid_components();
        req.pubkey = "!!!not-base64!!!".to_string();
        let repo = MockIdentityRepo::new(); // not called — validation fails early
        let err = validate_add_device_request(&repo, account.id, &req)
            .await
            .err()
            .expect("expected error");
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_validate_add_device_request_invalid_pubkey_length() {
        let (mut req, account) = make_valid_components();
        req.pubkey = encode_base64url(&[1u8; 16]); // 16 bytes, not 32
        let repo = MockIdentityRepo::new();
        let err = validate_add_device_request(&repo, account.id, &req)
            .await
            .err()
            .expect("expected error");
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_validate_add_device_request_empty_name() {
        let (mut req, account) = make_valid_components();
        req.name = String::new();
        let repo = MockIdentityRepo::new();
        let err = validate_add_device_request(&repo, account.id, &req)
            .await
            .err()
            .expect("expected error");
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_validate_add_device_request_invalid_cert_length() {
        let (mut req, account) = make_valid_components();
        req.certificate = encode_base64url(&[0u8; 32]); // 32 bytes, not 64
        let repo = MockIdentityRepo::new();
        let err = validate_add_device_request(&repo, account.id, &req)
            .await
            .err()
            .expect("expected error");
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_validate_add_device_request_wrong_signature() {
        let (mut req, account) = make_valid_components();
        req.certificate = encode_base64url(&[0xFFu8; 64]); // valid length, wrong bytes
        let repo = mock_with_account(account.clone());
        let err = validate_add_device_request(&repo, account.id, &req)
            .await
            .err()
            .expect("expected error");
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
    }

    // ── get_owned_device ────────────────────────────────────────────────────

    fn make_device_record(account_id: Uuid) -> DeviceKeyRecord {
        DeviceKeyRecord {
            id: Uuid::new_v4(),
            account_id,
            device_kid: Kid::derive(&[3u8; 32]),
            device_pubkey: encode_base64url(&[0u8; 32]),
            device_name: "Test Device".to_string(),
            certificate: vec![],
            last_used_at: None,
            revoked_at: None,
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_get_owned_device_not_found() {
        let repo = MockIdentityRepo::new(); // default: get_device_key_by_kid returns NotFound
        let kid = Kid::derive(&[0u8; 32]);
        let err = get_owned_device(&repo, &kid, Uuid::new_v4())
            .await
            .err()
            .expect("expected error");
        assert_eq!(err.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_get_owned_device_wrong_account_returns_not_found() {
        // Security: cross-account access must return 404, not 403 — no device enumeration.
        let owner_id = Uuid::new_v4();
        let caller_id = Uuid::new_v4();
        let record = make_device_record(owner_id);
        let kid = record.device_kid.clone();

        let repo = MockIdentityRepo::new();
        repo.set_get_device_key_by_kid_result(Ok(record));

        let err = get_owned_device(&repo, &kid, caller_id)
            .await
            .err()
            .expect("expected error");
        assert_eq!(err.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_get_owned_device_database_error_returns_internal() {
        let repo = MockIdentityRepo::new();
        repo.set_get_device_key_by_kid_result(Err(DeviceKeyRepoError::Database(
            sqlx::Error::Protocol("db error".to_string()),
        )));
        let kid = Kid::derive(&[0u8; 32]);
        let err = get_owned_device(&repo, &kid, Uuid::new_v4())
            .await
            .err()
            .expect("expected error");
        assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn test_get_owned_device_matching_account_returns_record() {
        let account_id = Uuid::new_v4();
        let record = make_device_record(account_id);
        let kid = record.device_kid.clone();

        let repo = MockIdentityRepo::new();
        repo.set_get_device_key_by_kid_result(Ok(record.clone()));

        let result = get_owned_device(&repo, &kid, account_id).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().device_kid, kid);
    }

    #[tokio::test]
    async fn test_validate_add_device_request_account_not_found() {
        // Post-auth account lookup fails → internal error (server-side invariant violation)
        let (req, account) = make_valid_components();
        let repo = MockIdentityRepo::new(); // default: get_account_by_id returns NotFound
        let err = validate_add_device_request(&repo, account.id, &req)
            .await
            .err()
            .expect("expected error");
        assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    // ── revoke_device error paths ────────────────────────────────────────────

    /// Build an `AuthenticatedDevice` and a repo where `get_owned_device` succeeds
    /// (so we reach the `revoke_device_key` call), then inject an error there.
    fn setup_revoke_preconditions(
        account_id: Uuid,
        target_kid: &Kid,
    ) -> (std::sync::Arc<MockIdentityRepo>, AuthenticatedDevice) {
        // auth device has a different KID so the "cannot revoke self" check passes
        let auth_kid = Kid::derive(&[0xAAu8; 32]);
        let record = make_device_record(account_id);

        let repo = std::sync::Arc::new(MockIdentityRepo::new());
        // `get_owned_device` calls `get_device_key_by_kid` — return a record owned by this account
        repo.set_get_device_key_by_kid_result(Ok(record));

        let auth = AuthenticatedDevice::for_test(account_id, auth_kid, axum::body::Bytes::new());
        let _ = target_kid; // used by caller for the Path argument
        (repo, auth)
    }

    #[tokio::test]
    async fn test_revoke_device_already_revoked_returns_conflict() {
        use axum::response::IntoResponse;
        use axum::{body::to_bytes, extract::Extension, extract::Path};

        let account_id = Uuid::new_v4();
        let target_kid = Kid::derive(&[0xBBu8; 32]);
        let (repo, auth) = setup_revoke_preconditions(account_id, &target_kid);
        repo.set_revoke_device_key_result(Err(DeviceKeyRepoError::AlreadyRevoked));

        let response = revoke_device(
            Extension(repo as std::sync::Arc<dyn crate::identity::repo::IdentityRepo>),
            Path(target_kid.as_str().to_string()),
            auth,
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body = to_bytes(response.into_body(), 1024).await.expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(payload["error"].as_str().unwrap(), "Device already revoked");
    }

    #[tokio::test]
    async fn test_revoke_device_db_error_returns_internal() {
        use axum::response::IntoResponse;
        use axum::{extract::Extension, extract::Path};

        let account_id = Uuid::new_v4();
        let target_kid = Kid::derive(&[0xBBu8; 32]);
        let (repo, auth) = setup_revoke_preconditions(account_id, &target_kid);
        repo.set_revoke_device_key_result(Err(DeviceKeyRepoError::Database(
            sqlx::Error::Protocol("db error".to_string()),
        )));

        let response = revoke_device(
            Extension(repo as std::sync::Arc<dyn crate::identity::repo::IdentityRepo>),
            Path(target_kid.as_str().to_string()),
            auth,
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    // ── rename_device error paths ────────────────────────────────────────────

    fn setup_rename_preconditions(
        account_id: Uuid,
        target_kid: &Kid,
    ) -> (std::sync::Arc<MockIdentityRepo>, AuthenticatedDevice) {
        let auth_kid = Kid::derive(&[0xAAu8; 32]);
        let record = make_device_record(account_id);

        let repo = std::sync::Arc::new(MockIdentityRepo::new());
        repo.set_get_device_key_by_kid_result(Ok(record));

        let body = axum::body::Bytes::from(r#"{"name":"Renamed Device"}"#);
        let auth = AuthenticatedDevice::for_test(account_id, auth_kid, body);
        let _ = target_kid;
        (repo, auth)
    }

    #[tokio::test]
    async fn test_rename_device_already_revoked_returns_conflict() {
        use axum::response::IntoResponse;
        use axum::{body::to_bytes, extract::Extension, extract::Path};

        let account_id = Uuid::new_v4();
        let target_kid = Kid::derive(&[0xCCu8; 32]);
        let (repo, auth) = setup_rename_preconditions(account_id, &target_kid);
        repo.set_rename_device_key_result(Err(DeviceKeyRepoError::AlreadyRevoked));

        let response = rename_device(
            Extension(repo as std::sync::Arc<dyn crate::identity::repo::IdentityRepo>),
            Path(target_kid.as_str().to_string()),
            auth,
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body = to_bytes(response.into_body(), 1024).await.expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(
            payload["error"].as_str().unwrap(),
            "Cannot rename a revoked device"
        );
    }

    #[tokio::test]
    async fn test_rename_device_db_error_returns_internal() {
        use axum::response::IntoResponse;
        use axum::{extract::Extension, extract::Path};

        let account_id = Uuid::new_v4();
        let target_kid = Kid::derive(&[0xCCu8; 32]);
        let (repo, auth) = setup_rename_preconditions(account_id, &target_kid);
        repo.set_rename_device_key_result(Err(DeviceKeyRepoError::Database(
            sqlx::Error::Protocol("db error".to_string()),
        )));

        let response = rename_device(
            Extension(repo as std::sync::Arc<dyn crate::identity::repo::IdentityRepo>),
            Path(target_kid.as_str().to_string()),
            auth,
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
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
            return Err(super::not_found("Device not found"));
        }
        Err(e) => {
            tracing::error!("Failed to look up device: {e}");
            return Err(super::internal_error());
        }
    };

    if device.account_id != account_id {
        return Err(super::not_found("Device not found"));
    }

    Ok(device)
}
