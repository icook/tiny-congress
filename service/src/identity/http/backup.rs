//! Backup retrieval endpoint for login/recovery flow.

use axum::{extract::Extension, extract::Path, http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;
use sqlx::PgPool;
use tc_crypto::Kid;

use super::ErrorResponse;
use crate::identity::repo::{
    get_account_by_username, AccountRepoError, BackupRepo, BackupRepoError, PgBackupRepo,
};

#[derive(Debug, Serialize)]
pub struct BackupResponse {
    pub encrypted_backup: String,
    pub root_kid: Kid,
}

/// GET /auth/backup/{username} — fetch encrypted backup for login
pub async fn get_backup(
    Extension(pool): Extension<PgPool>,
    Path(username): Path<String>,
) -> impl IntoResponse {
    let username = username.trim();
    if username.is_empty() {
        return super::bad_request("Username cannot be empty");
    }

    let account = match get_account_by_username(&pool, username).await {
        Ok(a) => a,
        Err(AccountRepoError::NotFound) => return not_found("Account not found"),
        Err(e) => {
            tracing::error!("Failed to look up account: {e}");
            return internal_error();
        }
    };

    let backup_repo = PgBackupRepo::new(pool);
    match backup_repo.get_by_kid(&account.root_kid).await {
        Ok(record) => {
            let encrypted_backup = tc_crypto::encode_base64url(&record.encrypted_backup);
            (
                StatusCode::OK,
                Json(BackupResponse {
                    encrypted_backup,
                    root_kid: account.root_kid,
                }),
            )
                .into_response()
        }
        Err(BackupRepoError::NotFound) => not_found("Backup not found"),
        Err(e) => {
            tracing::error!("Failed to fetch backup: {e}");
            internal_error()
        }
    }
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
