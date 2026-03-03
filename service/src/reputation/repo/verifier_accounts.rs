//! Verifier account persistence operations

use chrono::{DateTime, Utc};
use uuid::Uuid;

// ─── Record types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct VerifierAccountRecord {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct CreatedVerifierAccount {
    pub id: Uuid,
    pub name: String,
}

// ─── Error type ────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum VerifierAccountRepoError {
    #[error("verifier account name already exists")]
    DuplicateName,
    #[error("verifier account not found")]
    NotFound,
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

// ─── SQL row types ─────────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct VerifierAccountRow {
    id: Uuid,
    name: String,
    description: Option<String>,
    created_at: DateTime<Utc>,
}

fn row_to_record(row: VerifierAccountRow) -> VerifierAccountRecord {
    VerifierAccountRecord {
        id: row.id,
        name: row.name,
        description: row.description,
        created_at: row.created_at,
    }
}

// ─── SQL operations ────────────────────────────────────────────────────────

/// Create or return existing verifier account (upsert by name).
/// Used at startup to ensure the verifier exists.
///
/// # Errors
///
/// Returns `Database` on connection or query failure.
pub async fn ensure_verifier_account<'e, E>(
    executor: E,
    name: &str,
    description: Option<&str>,
) -> Result<CreatedVerifierAccount, VerifierAccountRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let row = sqlx::query_as::<_, VerifierAccountRow>(
        r"
        INSERT INTO reputation__verifier_accounts (id, name, description)
        VALUES (gen_random_uuid(), $1, $2)
        ON CONFLICT (name) DO UPDATE SET name = EXCLUDED.name
        RETURNING id, name, description, created_at
        ",
    )
    .bind(name)
    .bind(description)
    .fetch_one(executor)
    .await?;

    Ok(CreatedVerifierAccount {
        id: row.id,
        name: row.name,
    })
}

/// # Errors
///
/// Returns `NotFound` if no verifier account exists with this name.
/// Returns `Database` on connection or query failure.
pub async fn get_verifier_account_by_name<'e, E>(
    executor: E,
    name: &str,
) -> Result<VerifierAccountRecord, VerifierAccountRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let row = sqlx::query_as::<_, VerifierAccountRow>(
        r"
        SELECT id, name, description, created_at
        FROM reputation__verifier_accounts
        WHERE name = $1
        ",
    )
    .bind(name)
    .fetch_optional(executor)
    .await?;

    row.map_or_else(
        || Err(VerifierAccountRepoError::NotFound),
        |r| Ok(row_to_record(r)),
    )
}
