//! Glicko-2 rating persistence operations for the ranking engine.

use uuid::Uuid;

// ─── Types ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RatingRecord {
    pub submission_id: Uuid,
    pub rating: f64,
    pub deviation: f64,
    pub volatility: f64,
    pub matchup_count: i32,
}

// ─── Operations ─────────────────────────────────────────────────────────────

/// Bulk-insert default Glicko-2 ratings for a set of submissions.
///
/// Uses `ON CONFLICT DO NOTHING` so it is safe to call after some ratings
/// already exist (idempotent for existing rows).
///
/// # Errors
///
/// Returns `sqlx::Error` on connection failure.
pub async fn initialize_ratings<'e, E>(
    executor: E,
    submission_ids: &[Uuid],
) -> Result<(), sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    if submission_ids.is_empty() {
        return Ok(());
    }

    // Build a multi-row VALUES list via unnest for efficiency.
    sqlx::query(
        r"
        INSERT INTO rooms__ratings (submission_id)
        SELECT unnest($1::uuid[])
        ON CONFLICT (submission_id) DO NOTHING
        ",
    )
    .bind(submission_ids)
    .execute(executor)
    .await?;

    Ok(())
}

/// Fetch the rating record for a single submission. Returns `None` if not found.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection failure.
pub async fn get_rating<'e, E>(
    executor: E,
    submission_id: Uuid,
) -> Result<Option<RatingRecord>, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_as::<_, RatingRecord>(
        r"
        SELECT submission_id, rating, deviation, volatility, matchup_count
        FROM rooms__ratings
        WHERE submission_id = $1
        ",
    )
    .bind(submission_id)
    .fetch_optional(executor)
    .await
}

/// Return all rating records for submissions in a round, ordered by rating DESC.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection failure.
pub async fn get_ratings_for_round<'e, E>(
    executor: E,
    round_id: Uuid,
) -> Result<Vec<RatingRecord>, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_as::<_, RatingRecord>(
        r"
        SELECT r.submission_id, r.rating, r.deviation, r.volatility, r.matchup_count
        FROM rooms__ratings r
        JOIN rooms__submissions s ON s.id = r.submission_id
        WHERE s.round_id = $1
        ORDER BY r.rating DESC
        ",
    )
    .bind(round_id)
    .fetch_all(executor)
    .await
}

/// Overwrite the Glicko-2 state for a submission.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection failure.
pub async fn update_rating<'e, E>(
    executor: E,
    submission_id: Uuid,
    rating: f64,
    deviation: f64,
    volatility: f64,
    matchup_count: i32,
) -> Result<(), sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query(
        r"
        UPDATE rooms__ratings
        SET rating = $1, deviation = $2, volatility = $3, matchup_count = $4
        WHERE submission_id = $5
        ",
    )
    .bind(rating)
    .bind(deviation)
    .bind(volatility)
    .bind(matchup_count)
    .bind(submission_id)
    .execute(executor)
    .await?;

    Ok(())
}
