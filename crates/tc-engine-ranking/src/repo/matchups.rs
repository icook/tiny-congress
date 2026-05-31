//! Matchup persistence operations for the ranking engine.

use chrono::{DateTime, Utc};
use uuid::Uuid;

// ─── Types ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MatchupRecord {
    pub id: Uuid,
    pub round_id: Uuid,
    pub ranker_id: Uuid,
    pub submission_a: Uuid,
    pub submission_b: Uuid,
    pub winner_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

// ─── Operations ─────────────────────────────────────────────────────────────

/// Create a pairwise matchup.
///
/// Enforces `submission_a < submission_b` ordering before insert to satisfy
/// the `chk_matchups_ordered_pair` constraint.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection failure or constraint violation
/// (e.g., duplicate `(round_id, ranker_id, submission_a, submission_b)`).
pub async fn create_matchup<'e, E>(
    executor: E,
    round_id: Uuid,
    ranker_id: Uuid,
    submission_a: Uuid,
    submission_b: Uuid,
    winner_id: Option<Uuid>,
) -> Result<MatchupRecord, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let (a, b) = ordered_pair(submission_a, submission_b);

    sqlx::query_as::<_, MatchupRecord>(
        r"
        INSERT INTO rooms__matchups (round_id, ranker_id, submission_a, submission_b, winner_id)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, round_id, ranker_id, submission_a, submission_b, winner_id, created_at
        ",
    )
    .bind(round_id)
    .bind(ranker_id)
    .bind(a)
    .bind(b)
    .bind(winner_id)
    .fetch_one(executor)
    .await
}

/// Return all `(submission_a, submission_b)` pairs already judged by a ranker in a round.
///
/// Pairs are returned in their stored (ordered) form: `submission_a < submission_b`.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection failure.
pub async fn get_judged_pairs<'e, E>(
    executor: E,
    round_id: Uuid,
    ranker_id: Uuid,
) -> Result<Vec<(Uuid, Uuid)>, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let rows: Vec<(Uuid, Uuid)> = sqlx::query_as(
        r"
        SELECT submission_a, submission_b
        FROM rooms__matchups
        WHERE round_id = $1
          AND ranker_id = $2
        ",
    )
    .bind(round_id)
    .bind(ranker_id)
    .fetch_all(executor)
    .await?;

    Ok(rows)
}

/// Count the number of matchups a ranker has completed in a round.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection failure.
pub async fn count_matchups_for_ranker<'e, E>(
    executor: E,
    round_id: Uuid,
    ranker_id: Uuid,
) -> Result<i64, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM rooms__matchups WHERE round_id = $1 AND ranker_id = $2",
    )
    .bind(round_id)
    .bind(ranker_id)
    .fetch_one(executor)
    .await?;

    Ok(count)
}

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Return `(a, b)` such that `a < b` (bytes-level UUID ordering).
/// This ensures the `chk_matchups_ordered_pair` constraint is satisfied.
fn ordered_pair(x: Uuid, y: Uuid) -> (Uuid, Uuid) {
    if x < y {
        (x, y)
    } else {
        (y, x)
    }
}
