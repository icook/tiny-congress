//! HTTP handlers for encrypted key backup

use std::sync::Arc;

use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::identity::repo::{BackupRepo, BackupRepoError};
use tc_crypto::{decode_base64url_native as decode_base64url, encode_base64url};

/// Create backup request payload
#[derive(Debug, Deserialize)]
pub struct CreateBackupRequest {
    /// Key ID (must match an existing account's `root_kid`)
    pub kid: String,
    /// Base64url-encoded encrypted backup envelope
    pub encrypted_backup: String,
}

/// Create backup response
#[derive(Debug, Serialize)]
pub struct CreateBackupResponse {
    pub kid: String,
    pub created_at: DateTime<Utc>,
}

/// Get backup response
#[derive(Debug, Serialize)]
pub struct GetBackupResponse {
    pub encrypted_backup: String,
    pub salt: String,
    pub kdf_algorithm: String,
    pub version: i32,
}

/// Error response
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

/// Parsed backup envelope metadata
struct EnvelopeMetadata {
    version: u8,
    kdf_algorithm: &'static str,
    salt: Vec<u8>,
}

/// Parse and validate backup envelope, extracting metadata
fn parse_envelope(backup_bytes: &[u8]) -> Result<EnvelopeMetadata, &'static str> {
    // Minimum: 1 + 1 + 4 + 16 + 12 + 48 = 82 bytes
    if backup_bytes.len() < 82 {
        return Err("Encrypted backup envelope too small");
    }

    let version = backup_bytes[0];
    if version != 1 {
        return Err("Unsupported backup version");
    }

    let kdf_id = backup_bytes[1];
    let kdf_algorithm = match kdf_id {
        1 => "argon2id",
        2 => "pbkdf2",
        _ => return Err("Unknown KDF algorithm"),
    };

    // Extract salt based on KDF type
    // Argon2: params at bytes 2-13 (12 bytes), salt at 14-29 (16 bytes)
    // PBKDF2: params at bytes 2-5 (4 bytes), salt at 6-21 (16 bytes)
    let salt_offset = if kdf_id == 1 { 14 } else { 6 };
    let salt = backup_bytes[salt_offset..salt_offset + 16].to_vec();

    Ok(EnvelopeMetadata {
        version,
        kdf_algorithm,
        salt,
    })
}

/// Convert `BackupRepoError` to HTTP response
fn backup_error_response(e: BackupRepoError) -> (StatusCode, Json<ErrorResponse>) {
    match e {
        BackupRepoError::DuplicateAccount => (
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: "Backup already exists for this account".to_string(),
            }),
        ),
        BackupRepoError::DuplicateKid => (
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: "Backup already exists for this key ID".to_string(),
            }),
        ),
        BackupRepoError::AccountNotFound => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Account not found".to_string(),
            }),
        ),
        BackupRepoError::NotFound => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Backup not found".to_string(),
            }),
        ),
        BackupRepoError::Database(db_err) => {
            tracing::error!("Backup operation failed: {}", db_err);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Internal server error".to_string(),
                }),
            )
        }
    }
}

/// Handle create backup request
///
/// POST /auth/backup
pub async fn create_backup(
    Extension(repo): Extension<Arc<dyn BackupRepo>>,
    Json(req): Json<CreateBackupRequest>,
) -> impl IntoResponse {
    // Validate KID format
    let kid = req.kid.trim();
    if kid.is_empty() || kid.len() > 64 {
        return bad_request("Invalid kid format");
    }

    // Decode encrypted backup envelope
    let Ok(backup_bytes) = decode_base64url(&req.encrypted_backup) else {
        return bad_request("Invalid base64url encoding for encrypted_backup");
    };

    // Parse envelope metadata
    let metadata = match parse_envelope(&backup_bytes) {
        Ok(m) => m,
        Err(msg) => return bad_request(msg),
    };

    // TODO: Add account lookup by kid or require account_id in request
    let account_id = uuid::Uuid::nil(); // Placeholder

    match repo
        .create(
            account_id,
            kid,
            &backup_bytes,
            &metadata.salt,
            metadata.kdf_algorithm,
            i32::from(metadata.version),
        )
        .await
    {
        Ok(created) => (
            StatusCode::CREATED,
            Json(CreateBackupResponse {
                kid: created.kid,
                created_at: created.created_at,
            }),
        )
            .into_response(),
        Err(e) => backup_error_response(e).into_response(),
    }
}

/// Helper to create bad request response
fn bad_request(msg: &str) -> axum::response::Response {
    (
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
            error: msg.to_string(),
        }),
    )
        .into_response()
}

/// Handle get backup request
///
/// GET /auth/backup/:kid
pub async fn get_backup(
    Extension(repo): Extension<Arc<dyn BackupRepo>>,
    Path(kid): Path<String>,
) -> impl IntoResponse {
    match repo.get_by_kid(&kid).await {
        Ok(backup) => (
            StatusCode::OK,
            Json(GetBackupResponse {
                encrypted_backup: encode_base64url(&backup.encrypted_backup),
                salt: encode_base64url(&backup.salt),
                kdf_algorithm: backup.kdf_algorithm,
                version: backup.version,
            }),
        )
            .into_response(),
        Err(e) => backup_error_response(e).into_response(),
    }
}

/// Handle delete backup request
///
/// DELETE /auth/backup/:kid
pub async fn delete_backup(
    Extension(repo): Extension<Arc<dyn BackupRepo>>,
    Path(kid): Path<String>,
) -> impl IntoResponse {
    // TODO: Add authentication - require signed envelope proving ownership
    match repo.delete_by_kid(&kid).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => backup_error_response(e).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::repo::mock::MockBackupRepo;
    use crate::identity::repo::{BackupRecord, CreatedBackup};
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        routing::{delete, get, post},
        Router,
    };
    use chrono::Utc;
    use tower::ServiceExt;
    use uuid::Uuid;

    fn test_router(repo: Arc<dyn BackupRepo>) -> Router {
        Router::new()
            .route("/auth/backup", post(create_backup))
            .route("/auth/backup/{kid}", get(get_backup))
            .route("/auth/backup/{kid}", delete(delete_backup))
            .layer(Extension(repo))
    }

    #[tokio::test]
    async fn test_get_backup_success() {
        let mock_repo = Arc::new(MockBackupRepo::new());
        mock_repo.set_get_result(Ok(BackupRecord {
            id: Uuid::new_v4(),
            account_id: Uuid::new_v4(),
            kid: "test-kid".to_string(),
            encrypted_backup: vec![1u8; 100],
            salt: vec![2u8; 16],
            kdf_algorithm: "argon2id".to_string(),
            version: 1,
            created_at: Utc::now(),
        }));
        let app = test_router(mock_repo);

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/auth/backup/test-kid")
                    .body(Body::empty())
                    .expect("request builder"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_get_backup_not_found() {
        let mock_repo = Arc::new(MockBackupRepo::new());
        mock_repo.set_get_result(Err(BackupRepoError::NotFound));
        let app = test_router(mock_repo);

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/auth/backup/nonexistent")
                    .body(Body::empty())
                    .expect("request builder"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_delete_backup_success() {
        let mock_repo = Arc::new(MockBackupRepo::new());
        mock_repo.set_delete_result(Ok(()));
        let app = test_router(mock_repo);

        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/auth/backup/test-kid")
                    .body(Body::empty())
                    .expect("request builder"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn test_delete_backup_not_found() {
        let mock_repo = Arc::new(MockBackupRepo::new());
        mock_repo.set_delete_result(Err(BackupRepoError::NotFound));
        let app = test_router(mock_repo);

        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/auth/backup/nonexistent")
                    .body(Body::empty())
                    .expect("request builder"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
