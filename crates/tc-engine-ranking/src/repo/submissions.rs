//! Submission persistence operations for the ranking engine.

use chrono::{DateTime, Utc};
use uuid::Uuid;

// ─── Types ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, sqlx::Type)]
#[sqlx(type_name = "submission_content_type", rename_all = "snake_case")]
pub enum ContentType {
    Url,
    Image,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SubmissionRecord {
    pub id: Uuid,
    pub round_id: Uuid,
    pub author_id: Uuid,
    pub content_type: ContentType,
    pub url: Option<String>,
    pub image_key: Option<String>,
    pub caption: Option<String>,
    pub created_at: DateTime<Utc>,
}

// ─── Operations ─────────────────────────────────────────────────────────────

/// Create a new submission in the given round.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection failure or constraint violation
/// (e.g., duplicate `(round_id, author_id)`, or missing `url`/`image_key` check).
#[allow(clippy::too_many_arguments)]
pub async fn create_submission<'e, E>(
    executor: E,
    round_id: Uuid,
    author_id: Uuid,
    content_type: ContentType,
    url: Option<&str>,
    image_key: Option<&str>,
    caption: Option<&str>,
) -> Result<SubmissionRecord, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_as::<_, SubmissionRecord>(
        r"
        INSERT INTO rooms__submissions (round_id, author_id, content_type, url, image_key, caption)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, round_id, author_id, content_type, url, image_key, caption, created_at
        ",
    )
    .bind(round_id)
    .bind(author_id)
    .bind(content_type)
    .bind(url)
    .bind(image_key)
    .bind(caption)
    .fetch_one(executor)
    .await
}

/// Fetch a single submission by ID. Returns `None` if not found.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection failure.
pub async fn get_submission<'e, E>(
    executor: E,
    submission_id: Uuid,
) -> Result<Option<SubmissionRecord>, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_as::<_, SubmissionRecord>(
        r"
        SELECT id, round_id, author_id, content_type, url, image_key, caption, created_at
        FROM rooms__submissions
        WHERE id = $1
        ",
    )
    .bind(submission_id)
    .fetch_optional(executor)
    .await
}

/// Return all submissions for a round, ordered by `created_at` ASC.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection failure.
pub async fn list_submissions<'e, E>(
    executor: E,
    round_id: Uuid,
) -> Result<Vec<SubmissionRecord>, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_as::<_, SubmissionRecord>(
        r"
        SELECT id, round_id, author_id, content_type, url, image_key, caption, created_at
        FROM rooms__submissions
        WHERE round_id = $1
        ORDER BY created_at ASC
        ",
    )
    .bind(round_id)
    .fetch_all(executor)
    .await
}

/// Count the number of submissions for a round.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection failure.
pub async fn count_submissions<'e, E>(executor: E, round_id: Uuid) -> Result<i64, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM rooms__submissions WHERE round_id = $1")
            .bind(round_id)
            .fetch_one(executor)
            .await?;

    Ok(count)
}

/// Check whether a given author has already submitted in this round.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection failure.
pub async fn has_submitted<'e, E>(
    executor: E,
    round_id: Uuid,
    author_id: Uuid,
) -> Result<bool, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let row: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM rooms__submissions WHERE round_id = $1 AND author_id = $2")
            .bind(round_id)
            .bind(author_id)
            .fetch_optional(executor)
            .await?;

    Ok(row.is_some())
}
