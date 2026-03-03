//! Vote persistence and aggregation operations

use chrono::{DateTime, Utc};
use uuid::Uuid;

// ─── Record types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct VoteRecord {
    pub id: Uuid,
    pub poll_id: Uuid,
    pub dimension_id: Uuid,
    pub user_id: Uuid,
    pub value: f32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct DimensionStats {
    pub dimension_id: Uuid,
    pub dimension_name: String,
    pub count: i64,
    pub mean: f64,
    pub median: f64,
    pub stddev: f64,
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, thiserror::Error)]
pub enum VoteRepoError {
    #[error("vote not found")]
    NotFound,
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

// ─── SQL row types ─────────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct VoteRow {
    id: Uuid,
    poll_id: Uuid,
    dimension_id: Uuid,
    user_id: Uuid,
    value: f32,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct StatsRow {
    dimension_id: Uuid,
    dimension_name: String,
    vote_count: i64,
    vote_mean: Option<f64>,
    vote_stddev: Option<f64>,
    vote_min: Option<f64>,
    vote_max: Option<f64>,
}

// ─── Vote operations ──────────────────────────────────────────────────────

/// Upsert a vote (one per user per dimension). Updates value if already exists.
///
/// # Errors
///
/// Returns `Database` on connection failure.
pub async fn upsert_vote<'e, E>(
    executor: E,
    poll_id: Uuid,
    dimension_id: Uuid,
    user_id: Uuid,
    value: f32,
) -> Result<VoteRecord, VoteRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let row = sqlx::query_as::<_, VoteRow>(
        r"
        INSERT INTO rooms__votes (poll_id, dimension_id, user_id, value)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (poll_id, dimension_id, user_id)
        DO UPDATE SET value = EXCLUDED.value, updated_at = now()
        RETURNING id, poll_id, dimension_id, user_id, value, created_at, updated_at
        ",
    )
    .bind(poll_id)
    .bind(dimension_id)
    .bind(user_id)
    .bind(value)
    .fetch_one(executor)
    .await?;

    Ok(VoteRecord {
        id: row.id,
        poll_id: row.poll_id,
        dimension_id: row.dimension_id,
        user_id: row.user_id,
        value: row.value,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

/// Get a user's votes for a specific poll.
///
/// # Errors
///
/// Returns `Database` on connection failure.
pub async fn get_user_votes<'e, E>(
    executor: E,
    poll_id: Uuid,
    user_id: Uuid,
) -> Result<Vec<VoteRecord>, VoteRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let rows = sqlx::query_as::<_, VoteRow>(
        r"
        SELECT id, poll_id, dimension_id, user_id, value, created_at, updated_at
        FROM rooms__votes WHERE poll_id = $1 AND user_id = $2
        ",
    )
    .bind(poll_id)
    .bind(user_id)
    .fetch_all(executor)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| VoteRecord {
            id: r.id,
            poll_id: r.poll_id,
            dimension_id: r.dimension_id,
            user_id: r.user_id,
            value: r.value,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
        .collect())
}

/// Count unique voters for a poll.
///
/// # Errors
///
/// Returns `Database` on connection failure.
pub async fn count_voters<'e, E>(executor: E, poll_id: Uuid) -> Result<i64, VoteRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let count: i64 =
        sqlx::query_scalar(r"SELECT COUNT(DISTINCT user_id) FROM rooms__votes WHERE poll_id = $1")
            .bind(poll_id)
            .fetch_one(executor)
            .await?;

    Ok(count)
}

// ─── Aggregation ──────────────────────────────────────────────────────────

/// Compute per-dimension statistics for a poll. Median is computed with
/// `percentile_cont(0.5)` which requires the `ordered-set` aggregate.
///
/// # Errors
///
/// Returns `Database` on connection failure.
pub async fn compute_poll_stats(
    pool: &sqlx::PgPool,
    poll_id: Uuid,
) -> Result<Vec<DimensionStats>, VoteRepoError> {
    let rows = sqlx::query_as::<_, StatsRow>(
        r"
        SELECT
            d.id AS dimension_id,
            d.name AS dimension_name,
            COUNT(v.id) AS vote_count,
            AVG(v.value::float8) AS vote_mean,
            STDDEV_POP(v.value::float8) AS vote_stddev,
            MIN(v.value::float8) AS vote_min,
            MAX(v.value::float8) AS vote_max
        FROM rooms__poll_dimensions d
        LEFT JOIN rooms__votes v ON v.dimension_id = d.id AND v.poll_id = $1
        WHERE d.poll_id = $1
        GROUP BY d.id, d.name, d.sort_order
        ORDER BY d.sort_order ASC
        ",
    )
    .bind(poll_id)
    .fetch_all(pool)
    .await?;

    // Fetch medians separately (percentile_cont requires a separate query pattern)
    let medians = sqlx::query_as::<_, MedianRow>(
        r"
        SELECT
            d.id AS dimension_id,
            COALESCE(PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY v.value::float8), 0) AS median_value
        FROM rooms__poll_dimensions d
        LEFT JOIN rooms__votes v ON v.dimension_id = d.id AND v.poll_id = $1
        WHERE d.poll_id = $1
        GROUP BY d.id
        ",
    )
    .bind(poll_id)
    .fetch_all(pool)
    .await?;

    let median_map: std::collections::HashMap<Uuid, f64> = medians
        .into_iter()
        .map(|m| (m.dimension_id, m.median_value))
        .collect();

    Ok(rows
        .into_iter()
        .map(|r| DimensionStats {
            dimension_id: r.dimension_id,
            dimension_name: r.dimension_name,
            count: r.vote_count,
            mean: r.vote_mean.unwrap_or(0.0),
            median: median_map.get(&r.dimension_id).copied().unwrap_or(0.0),
            stddev: r.vote_stddev.unwrap_or(0.0),
            min: r.vote_min.unwrap_or(0.0),
            max: r.vote_max.unwrap_or(0.0),
        })
        .collect())
}

#[derive(sqlx::FromRow)]
struct MedianRow {
    dimension_id: Uuid,
    median_value: f64,
}
