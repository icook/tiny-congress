//! Research suggestion persistence operations

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct SuggestionRecord {
    pub id: Uuid,
    pub room_id: Uuid,
    pub poll_id: Uuid,
    pub account_id: Uuid,
    pub suggestion_text: String,
    pub status: String,
    pub filter_reason: Option<String>,
    pub evidence_ids: Vec<Uuid>,
    pub created_at: DateTime<Utc>,
    pub processed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, thiserror::Error)]
pub enum SuggestionRepoError {
    #[error("suggestion not found")]
    NotFound,
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

#[derive(sqlx::FromRow)]
struct SuggestionRow {
    id: Uuid,
    room_id: Uuid,
    poll_id: Uuid,
    account_id: Uuid,
    suggestion_text: String,
    status: String,
    filter_reason: Option<String>,
    evidence_ids: Vec<Uuid>,
    created_at: DateTime<Utc>,
    processed_at: Option<DateTime<Utc>>,
}

fn row_to_record(row: SuggestionRow) -> SuggestionRecord {
    SuggestionRecord {
        id: row.id,
        room_id: row.room_id,
        poll_id: row.poll_id,
        account_id: row.account_id,
        suggestion_text: row.suggestion_text,
        status: row.status,
        filter_reason: row.filter_reason,
        evidence_ids: row.evidence_ids,
        created_at: row.created_at,
        processed_at: row.processed_at,
    }
}

/// # Errors
///
/// Returns `Database` on connection failure.
pub async fn create_suggestion<'e, E>(
    executor: E,
    room_id: Uuid,
    poll_id: Uuid,
    account_id: Uuid,
    suggestion_text: &str,
    status: &str,
    filter_reason: Option<&str>,
) -> Result<SuggestionRecord, SuggestionRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let row = sqlx::query_as::<_, SuggestionRow>(
        r"
        INSERT INTO rooms__research_suggestions
            (room_id, poll_id, account_id, suggestion_text, status, filter_reason)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, room_id, poll_id, account_id, suggestion_text, status, filter_reason,
                  evidence_ids, created_at, processed_at
        ",
    )
    .bind(room_id)
    .bind(poll_id)
    .bind(account_id)
    .bind(suggestion_text)
    .bind(status)
    .bind(filter_reason)
    .fetch_one(executor)
    .await?;

    Ok(row_to_record(row))
}

/// Returns suggestions for a poll, newest first.
///
/// # Errors
///
/// Returns `Database` on connection failure.
pub async fn list_suggestions<'e, E>(
    executor: E,
    poll_id: Uuid,
) -> Result<Vec<SuggestionRecord>, SuggestionRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let rows = sqlx::query_as::<_, SuggestionRow>(
        r"
        SELECT id, room_id, poll_id, account_id, suggestion_text, status, filter_reason,
               evidence_ids, created_at, processed_at
        FROM rooms__research_suggestions
        WHERE poll_id = $1
        ORDER BY created_at DESC
        ",
    )
    .bind(poll_id)
    .fetch_all(executor)
    .await?;

    Ok(rows.into_iter().map(row_to_record).collect())
}

/// Returns the number of suggestions submitted by a user in a room today (UTC).
///
/// # Errors
///
/// Returns `Database` on connection failure.
pub async fn count_user_suggestions_today<'e, E>(
    executor: E,
    room_id: Uuid,
    account_id: Uuid,
) -> Result<i64, SuggestionRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let count: i64 = sqlx::query_scalar(
        r"
        SELECT COUNT(*)
        FROM rooms__research_suggestions
        WHERE room_id = $1
          AND account_id = $2
          AND created_at >= date_trunc('day', now() AT TIME ZONE 'UTC')
        ",
    )
    .bind(room_id)
    .bind(account_id)
    .fetch_one(executor)
    .await?;

    Ok(count)
}

/// Atomically claims the next queued suggestion using `FOR UPDATE SKIP LOCKED`.
///
/// Returns `None` if no queued suggestions are available.
///
/// # Errors
///
/// Returns `Database` on connection failure.
pub async fn claim_next_queued(
    pool: &PgPool,
) -> Result<Option<SuggestionRecord>, SuggestionRepoError> {
    let mut tx = pool.begin().await?;

    let row = sqlx::query_as::<_, SuggestionRow>(
        r"
        SELECT id, room_id, poll_id, account_id, suggestion_text, status, filter_reason,
               evidence_ids, created_at, processed_at
        FROM rooms__research_suggestions
        WHERE status = 'queued'
        ORDER BY created_at ASC
        LIMIT 1
        FOR UPDATE SKIP LOCKED
        ",
    )
    .fetch_optional(&mut *tx)
    .await?;

    let Some(row) = row else {
        tx.rollback().await?;
        return Ok(None);
    };

    sqlx::query(r"UPDATE rooms__research_suggestions SET status = 'processing' WHERE id = $1")
        .bind(row.id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok(Some(row_to_record(row)))
}

/// Marks a suggestion as complete and records the evidence IDs produced.
///
/// # Errors
///
/// Returns `NotFound` if no suggestion exists with this ID.
pub async fn complete_suggestion<'e, E>(
    executor: E,
    suggestion_id: Uuid,
    evidence_ids: &[Uuid],
) -> Result<(), SuggestionRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let result = sqlx::query(
        r"
        UPDATE rooms__research_suggestions
        SET status = 'complete', evidence_ids = $2, processed_at = now()
        WHERE id = $1
        ",
    )
    .bind(suggestion_id)
    .bind(evidence_ids)
    .execute(executor)
    .await?;

    if result.rows_affected() == 0 {
        return Err(SuggestionRepoError::NotFound);
    }
    Ok(())
}

/// Marks a suggestion as failed and records the reason.
///
/// # Errors
///
/// Returns `NotFound` if no suggestion exists with this ID.
pub async fn fail_suggestion<'e, E>(
    executor: E,
    suggestion_id: Uuid,
    reason: &str,
) -> Result<(), SuggestionRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let result = sqlx::query(
        r"
        UPDATE rooms__research_suggestions
        SET status = 'failed', filter_reason = $2, processed_at = now()
        WHERE id = $1
        ",
    )
    .bind(suggestion_id)
    .bind(reason)
    .execute(executor)
    .await?;

    if result.rows_affected() == 0 {
        return Err(SuggestionRepoError::NotFound);
    }
    Ok(())
}
