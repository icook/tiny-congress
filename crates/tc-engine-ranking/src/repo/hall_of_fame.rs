//! Hall of fame persistence operations for the ranking engine.

use chrono::{DateTime, Utc};
use uuid::Uuid;

// ─── Types ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct HallOfFameRecord {
    pub id: Uuid,
    pub room_id: Uuid,
    pub round_id: Uuid,
    pub submission_id: Uuid,
    pub final_rating: f64,
    pub rank: i32,
    pub created_at: DateTime<Utc>,
}

// ─── Operations ─────────────────────────────────────────────────────────────

/// Insert the top-ranked submissions from a closed round into the hall of fame.
///
/// `winners` is a slice of `(submission_id, final_rating, rank)` tuples.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection failure.
pub async fn insert_winners(
    pool: &sqlx::PgPool,
    room_id: Uuid,
    round_id: Uuid,
    winners: &[(Uuid, f64, i32)],
) -> Result<(), sqlx::Error> {
    if winners.is_empty() {
        return Ok(());
    }

    // Use a transaction to batch all inserts atomically.
    let mut tx = pool.begin().await?;

    for (submission_id, final_rating, rank) in winners {
        sqlx::query(
            r"
            INSERT INTO rooms__hall_of_fame (room_id, round_id, submission_id, final_rating, rank)
            VALUES ($1, $2, $3, $4, $5)
            ",
        )
        .bind(room_id)
        .bind(round_id)
        .bind(submission_id)
        .bind(final_rating)
        .bind(rank)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

/// Return hall of fame entries for a room, ordered by `created_at` DESC.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection failure.
pub async fn list_hall_of_fame<'e, E>(
    executor: E,
    room_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<HallOfFameRecord>, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_as::<_, HallOfFameRecord>(
        r"
        SELECT id, room_id, round_id, submission_id, final_rating, rank, created_at
        FROM rooms__hall_of_fame
        WHERE room_id = $1
        ORDER BY created_at DESC
        LIMIT $2
        OFFSET $3
        ",
    )
    .bind(room_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(executor)
    .await
}
