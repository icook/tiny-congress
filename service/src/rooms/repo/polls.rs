//! Poll and dimension persistence operations

use chrono::{DateTime, Utc};
use uuid::Uuid;

// ─── Record types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PollRecord {
    pub id: Uuid,
    pub room_id: Uuid,
    pub question: String,
    pub description: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub activated_at: Option<DateTime<Utc>>,
    pub closed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct DimensionRecord {
    pub id: Uuid,
    pub poll_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub min_value: f32,
    pub max_value: f32,
    pub sort_order: i32,
    pub min_label: Option<String>,
    pub max_label: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum PollRepoError {
    #[error("poll not found")]
    NotFound,
    #[error("dimension name already exists for this poll")]
    DuplicateDimension,
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

// ─── SQL row types ─────────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct PollRow {
    id: Uuid,
    room_id: Uuid,
    question: String,
    description: Option<String>,
    status: String,
    created_at: DateTime<Utc>,
    activated_at: Option<DateTime<Utc>>,
    closed_at: Option<DateTime<Utc>>,
}

#[derive(sqlx::FromRow)]
struct DimensionRow {
    id: Uuid,
    poll_id: Uuid,
    name: String,
    description: Option<String>,
    min_value: f32,
    max_value: f32,
    sort_order: i32,
    min_label: Option<String>,
    max_label: Option<String>,
}

fn poll_row_to_record(row: PollRow) -> PollRecord {
    PollRecord {
        id: row.id,
        room_id: row.room_id,
        question: row.question,
        description: row.description,
        status: row.status,
        created_at: row.created_at,
        activated_at: row.activated_at,
        closed_at: row.closed_at,
    }
}

fn dim_row_to_record(row: DimensionRow) -> DimensionRecord {
    DimensionRecord {
        id: row.id,
        poll_id: row.poll_id,
        name: row.name,
        description: row.description,
        min_value: row.min_value,
        max_value: row.max_value,
        sort_order: row.sort_order,
        min_label: row.min_label,
        max_label: row.max_label,
    }
}

// ─── Poll operations ──────────────────────────────────────────────────────

/// # Errors
///
/// Returns `Database` on connection failure.
pub async fn create_poll<'e, E>(
    executor: E,
    room_id: Uuid,
    question: &str,
    description: Option<&str>,
) -> Result<PollRecord, PollRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let row = sqlx::query_as::<_, PollRow>(
        r"
        INSERT INTO rooms__polls (room_id, question, description)
        VALUES ($1, $2, $3)
        RETURNING id, room_id, question, description, status, created_at, activated_at, closed_at
        ",
    )
    .bind(room_id)
    .bind(question)
    .bind(description)
    .fetch_one(executor)
    .await?;

    Ok(poll_row_to_record(row))
}

/// # Errors
///
/// Returns `Database` on connection failure.
pub async fn list_polls_by_room<'e, E>(
    executor: E,
    room_id: Uuid,
) -> Result<Vec<PollRecord>, PollRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let rows = sqlx::query_as::<_, PollRow>(
        r"
        SELECT id, room_id, question, description, status, created_at, activated_at, closed_at
        FROM rooms__polls WHERE room_id = $1 ORDER BY created_at ASC
        ",
    )
    .bind(room_id)
    .fetch_all(executor)
    .await?;

    Ok(rows.into_iter().map(poll_row_to_record).collect())
}

/// # Errors
///
/// Returns `NotFound` if no poll exists with this ID.
pub async fn get_poll<'e, E>(executor: E, poll_id: Uuid) -> Result<PollRecord, PollRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_as::<_, PollRow>(
        r"
        SELECT id, room_id, question, description, status, created_at, activated_at, closed_at
        FROM rooms__polls WHERE id = $1
        ",
    )
    .bind(poll_id)
    .fetch_optional(executor)
    .await?
    .map_or_else(
        || Err(PollRepoError::NotFound),
        |r| Ok(poll_row_to_record(r)),
    )
}

/// # Errors
///
/// Returns `NotFound` if no poll exists with this ID.
pub async fn update_poll_status<'e, E>(
    executor: E,
    poll_id: Uuid,
    status: &str,
) -> Result<(), PollRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let now = Utc::now();
    let result = sqlx::query(
        r"
        UPDATE rooms__polls
        SET status = $1,
            activated_at = CASE WHEN $1 = 'active' AND activated_at IS NULL THEN $2 ELSE activated_at END,
            closed_at = CASE WHEN $1 = 'closed' THEN $2 ELSE closed_at END
        WHERE id = $3
        ",
    )
    .bind(status)
    .bind(now)
    .bind(poll_id)
    .execute(executor)
    .await?;

    if result.rows_affected() == 0 {
        return Err(PollRepoError::NotFound);
    }
    Ok(())
}

// ─── Dimension operations ─────────────────────────────────────────────────

/// # Errors
///
/// Returns `DuplicateDimension` if a dimension with this name already exists for the poll.
#[allow(clippy::too_many_arguments)]
pub async fn create_dimension<'e, E>(
    executor: E,
    poll_id: Uuid,
    name: &str,
    description: Option<&str>,
    min_value: f32,
    max_value: f32,
    sort_order: i32,
    min_label: Option<&str>,
    max_label: Option<&str>,
) -> Result<DimensionRecord, PollRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let result = sqlx::query_as::<_, DimensionRow>(
        r"
        INSERT INTO rooms__poll_dimensions (poll_id, name, description, min_value, max_value, sort_order, min_label, max_label)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING id, poll_id, name, description, min_value, max_value, sort_order, min_label, max_label
        ",
    )
    .bind(poll_id)
    .bind(name)
    .bind(description)
    .bind(min_value)
    .bind(max_value)
    .bind(sort_order)
    .bind(min_label)
    .bind(max_label)
    .fetch_one(executor)
    .await;

    match result {
        Ok(row) => Ok(dim_row_to_record(row)),
        Err(e) => {
            if let sqlx::Error::Database(ref db_err) = e {
                if db_err.constraint() == Some("uq_poll_dimensions_poll_name") {
                    return Err(PollRepoError::DuplicateDimension);
                }
            }
            Err(PollRepoError::Database(e))
        }
    }
}

/// # Errors
///
/// Returns `Database` on connection failure.
pub async fn list_dimensions<'e, E>(
    executor: E,
    poll_id: Uuid,
) -> Result<Vec<DimensionRecord>, PollRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let rows = sqlx::query_as::<_, DimensionRow>(
        r"
        SELECT id, poll_id, name, description, min_value, max_value, sort_order, min_label, max_label
        FROM rooms__poll_dimensions WHERE poll_id = $1 ORDER BY sort_order ASC
        ",
    )
    .bind(poll_id)
    .fetch_all(executor)
    .await?;

    Ok(rows.into_iter().map(dim_row_to_record).collect())
}
