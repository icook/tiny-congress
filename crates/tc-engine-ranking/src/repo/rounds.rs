//! Round persistence operations for the ranking engine.

use chrono::{DateTime, Utc};
use uuid::Uuid;

// ─── Types ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, sqlx::Type)]
#[sqlx(type_name = "ranking_round_status", rename_all = "snake_case")]
pub enum RoundStatus {
    Submitting,
    Ranking,
    Closed,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RoundRecord {
    pub id: Uuid,
    pub room_id: Uuid,
    pub round_number: i32,
    pub submit_opens_at: DateTime<Utc>,
    pub rank_opens_at: DateTime<Utc>,
    pub closes_at: DateTime<Utc>,
    pub status: RoundStatus,
    pub created_at: DateTime<Utc>,
}

// ─── Operations ─────────────────────────────────────────────────────────────

/// Create a new round for the given room.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection failure or constraint violation
/// (e.g., duplicate `(room_id, round_number)`).
pub async fn create_round<'e, E>(
    executor: E,
    room_id: Uuid,
    round_number: i32,
    submit_opens_at: DateTime<Utc>,
    rank_opens_at: DateTime<Utc>,
    closes_at: DateTime<Utc>,
) -> Result<RoundRecord, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_as::<_, RoundRecord>(
        r"
        INSERT INTO rooms__rounds (room_id, round_number, submit_opens_at, rank_opens_at, closes_at)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, room_id, round_number, submit_opens_at, rank_opens_at, closes_at,
                  status, created_at
        ",
    )
    .bind(room_id)
    .bind(round_number)
    .bind(submit_opens_at)
    .bind(rank_opens_at)
    .bind(closes_at)
    .fetch_one(executor)
    .await
}

/// Fetch a single round by ID. Returns `None` if not found.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection failure.
pub async fn get_round<'e, E>(
    executor: E,
    round_id: Uuid,
) -> Result<Option<RoundRecord>, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_as::<_, RoundRecord>(
        r"
        SELECT id, room_id, round_number, submit_opens_at, rank_opens_at, closes_at,
               status, created_at
        FROM rooms__rounds
        WHERE id = $1
        ",
    )
    .bind(round_id)
    .fetch_optional(executor)
    .await
}

/// Return all non-closed rounds for a room, ordered by `round_number` ASC.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection failure.
pub async fn get_current_rounds<'e, E>(
    executor: E,
    room_id: Uuid,
) -> Result<Vec<RoundRecord>, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_as::<_, RoundRecord>(
        r"
        SELECT id, room_id, round_number, submit_opens_at, rank_opens_at, closes_at,
               status, created_at
        FROM rooms__rounds
        WHERE room_id = $1
          AND status != 'closed'
        ORDER BY round_number ASC
        ",
    )
    .bind(room_id)
    .fetch_all(executor)
    .await
}

/// Return all rounds for a room, ordered by `round_number` DESC.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection failure.
pub async fn list_rounds<'e, E>(executor: E, room_id: Uuid) -> Result<Vec<RoundRecord>, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_as::<_, RoundRecord>(
        r"
        SELECT id, room_id, round_number, submit_opens_at, rank_opens_at, closes_at,
               status, created_at
        FROM rooms__rounds
        WHERE room_id = $1
        ORDER BY round_number DESC
        ",
    )
    .bind(room_id)
    .fetch_all(executor)
    .await
}

/// Update the status of a round.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection failure.
pub async fn update_round_status<'e, E>(
    executor: E,
    round_id: Uuid,
    status: RoundStatus,
) -> Result<(), sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query(
        r"
        UPDATE rooms__rounds
        SET status = $1
        WHERE id = $2
        ",
    )
    .bind(status)
    .bind(round_id)
    .execute(executor)
    .await?;

    Ok(())
}

/// Return the highest `round_number` for a room, or 0 if no rounds exist.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection failure.
pub async fn get_latest_round_number<'e, E>(executor: E, room_id: Uuid) -> Result<i32, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let row: (Option<i32>,) =
        sqlx::query_as("SELECT MAX(round_number) FROM rooms__rounds WHERE room_id = $1")
            .bind(room_id)
            .fetch_one(executor)
            .await?;

    Ok(row.0.unwrap_or(0))
}
