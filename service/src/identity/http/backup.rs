//! Backup retrieval endpoint for login/recovery flow.

use std::sync::Arc;

use axum::{extract::Extension, extract::Path, http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;
use tc_crypto::Kid;

use crate::identity::repo::{AccountRepoError, BackupRepoError, IdentityRepo};

#[derive(Debug, Serialize)]
pub struct BackupResponse {
    pub encrypted_backup: String,
    pub root_kid: Kid,
}

/// GET /auth/backup/{username} — fetch encrypted backup for login
pub async fn get_backup(
    Extension(repo): Extension<Arc<dyn IdentityRepo>>,
    Path(username): Path<String>,
) -> impl IntoResponse {
    let username = username.trim();
    if username.is_empty() {
        return super::bad_request("Username cannot be empty");
    }

    let account = match repo.get_account_by_username(username).await {
        Ok(a) => a,
        Err(AccountRepoError::NotFound) => return super::not_found("Account not found"),
        Err(e) => {
            tracing::error!("Failed to look up account: {e}");
            return super::internal_error();
        }
    };

    match repo.get_backup_by_kid(&account.root_kid).await {
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
        Err(BackupRepoError::NotFound) => super::not_found("Backup not found"),
        Err(e) => {
            tracing::error!("Failed to fetch backup: {e}");
            super::internal_error()
        }
    }
}
