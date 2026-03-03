//! External identity link persistence operations

use chrono::{DateTime, Utc};
use uuid::Uuid;

// ─── Record types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ExternalIdentityRecord {
    pub id: Uuid,
    pub account_id: Uuid,
    pub provider: String,
    pub provider_subject: String,
    pub linked_at: DateTime<Utc>,
}

// ─── Error type ────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum ExternalIdentityRepoError {
    #[error("external identity already linked to another account")]
    AlreadyLinked,
    #[error("external identity not found")]
    NotFound,
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

// ─── SQL row types ─────────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct ExternalIdentityRow {
    id: Uuid,
    account_id: Uuid,
    provider: String,
    provider_subject: String,
    linked_at: DateTime<Utc>,
}

fn row_to_record(row: ExternalIdentityRow) -> ExternalIdentityRecord {
    ExternalIdentityRecord {
        id: row.id,
        account_id: row.account_id,
        provider: row.provider,
        provider_subject: row.provider_subject,
        linked_at: row.linked_at,
    }
}

// ─── SQL operations ────────────────────────────────────────────────────────

/// # Errors
///
/// Returns `AlreadyLinked` if this provider subject is already linked to another account.
/// Returns `Database` on connection or query failure.
pub async fn link_external_identity<'e, E>(
    executor: E,
    account_id: Uuid,
    provider: &str,
    provider_subject: &str,
) -> Result<ExternalIdentityRecord, ExternalIdentityRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let id = Uuid::new_v4();

    let result = sqlx::query_as::<_, ExternalIdentityRow>(
        r"
        INSERT INTO reputation__external_identities (id, account_id, provider, provider_subject)
        VALUES ($1, $2, $3, $4)
        RETURNING id, account_id, provider, provider_subject, linked_at
        ",
    )
    .bind(id)
    .bind(account_id)
    .bind(provider)
    .bind(provider_subject)
    .fetch_one(executor)
    .await;

    match result {
        Ok(row) => Ok(row_to_record(row)),
        Err(e) => {
            if let sqlx::Error::Database(ref db_err) = e {
                if let Some(constraint) = db_err.constraint() {
                    if constraint == "uq_external_identities_provider_subject" {
                        return Err(ExternalIdentityRepoError::AlreadyLinked);
                    }
                }
            }
            Err(ExternalIdentityRepoError::Database(e))
        }
    }
}

/// # Errors
///
/// Returns `NotFound` if no external identity link exists for this provider and subject.
/// Returns `Database` on connection or query failure.
pub async fn get_external_identity_by_provider<'e, E>(
    executor: E,
    provider: &str,
    provider_subject: &str,
) -> Result<ExternalIdentityRecord, ExternalIdentityRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let row = sqlx::query_as::<_, ExternalIdentityRow>(
        r"
        SELECT id, account_id, provider, provider_subject, linked_at
        FROM reputation__external_identities
        WHERE provider = $1 AND provider_subject = $2
        ",
    )
    .bind(provider)
    .bind(provider_subject)
    .fetch_optional(executor)
    .await?;

    row.map_or_else(
        || Err(ExternalIdentityRepoError::NotFound),
        |r| Ok(row_to_record(r)),
    )
}
