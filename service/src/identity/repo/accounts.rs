//! Account repository for database operations

use axum::{http::StatusCode, response::IntoResponse, Json};
use serde_json::json;
use tc_crypto::Kid;
use uuid::Uuid;

/// Account creation result
#[derive(Debug, Clone)]
pub struct CreatedAccount {
    pub id: Uuid,
    pub root_kid: Kid,
}

/// Error types for account operations
#[derive(Debug, thiserror::Error)]
pub enum AccountRepoError {
    #[error("username already taken")]
    DuplicateUsername,
    #[error("public key already registered")]
    DuplicateKey,
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

/// Create a new account with the given credentials.
///
/// Works with any sqlx executor (pool, connection, or transaction).
///
/// # Errors
///
/// Returns `AccountRepoError::DuplicateUsername` if username is taken.
/// Returns `AccountRepoError::DuplicateKey` if public key is already registered.
pub async fn create_account<'e, E>(
    executor: E,
    username: &str,
    root_pubkey: &str,
    root_kid: &Kid,
) -> Result<CreatedAccount, AccountRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let id = Uuid::new_v4();

    let result = sqlx::query(
        r"
        INSERT INTO accounts (id, username, root_pubkey, root_kid)
        VALUES ($1, $2, $3, $4)
        ",
    )
    .bind(id)
    .bind(username)
    .bind(root_pubkey)
    .bind(root_kid.as_str())
    .execute(executor)
    .await;

    match result {
        Ok(_) => Ok(CreatedAccount {
            id,
            root_kid: root_kid.clone(),
        }),
        Err(e) => {
            if let sqlx::Error::Database(db_err) = &e {
                if let Some(constraint) = db_err.constraint() {
                    match constraint {
                        "accounts_username_key" => return Err(AccountRepoError::DuplicateUsername),
                        "accounts_root_kid_key" => return Err(AccountRepoError::DuplicateKey),
                        _ => {}
                    }
                }
            }
            Err(AccountRepoError::Database(e))
        }
    }
}

impl IntoResponse for AccountRepoError {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::DuplicateUsername => (
                StatusCode::CONFLICT,
                Json(json!({ "error": "Username already taken" })),
            )
                .into_response(),
            Self::DuplicateKey => (
                StatusCode::CONFLICT,
                Json(json!({ "error": "Public key already registered" })),
            )
                .into_response(),
            Self::Database(db_err) => {
                tracing::error!("Signup failed (account): {db_err}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": "Internal server error" })),
                )
                    .into_response()
            }
        }
    }
}
