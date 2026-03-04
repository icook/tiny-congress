//! Vote persistence and aggregation operations

use chrono::{DateTime, Utc};
use uuid::Uuid;

// ─── Record types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct BucketCount {
    /// 1-indexed bucket number (1 = lowest range, 10 = highest)
    pub bucket: i32,
    pub count: i64,
}

#[derive(Debug, Clone)]
pub struct DimensionDistribution {
    pub dimension_id: uuid::Uuid,
    pub dimension_name: String,
    pub min_value: f32,
    pub max_value: f32,
    /// Always exactly 10 entries, one per bucket (count may be 0)
    pub buckets: Vec<BucketCount>,
}

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

#[derive(sqlx::FromRow)]
#[allow(dead_code)] // fields populated by sqlx via FromRow; rustc can't see macro usage
struct DistributionRow {
    dimension_id: uuid::Uuid,
    dimension_name: String,
    min_value: f32,
    max_value: f32,
    bucket: i32,
    count: i64,
}

/// Compute per-dimension vote distribution for a poll, bucketed into 10 bins.
///
/// Uses `width_bucket()` to assign each vote to a bin across the dimension's
/// [`min_value`, `max_value`] range. Votes exactly at `max_value` are clamped into
/// bucket 10. Dimensions with no votes return all-zero bucket counts.
///
/// # Errors
///
/// Returns `Database` on connection failure.
pub async fn compute_poll_distribution(
    pool: &sqlx::PgPool,
    poll_id: uuid::Uuid,
) -> Result<Vec<DimensionDistribution>, VoteRepoError> {
    const NUM_BUCKETS: i32 = 10;

    let rows = sqlx::query_as::<_, DistributionRow>(
        r"
        SELECT
            d.id AS dimension_id,
            d.name AS dimension_name,
            d.min_value,
            d.max_value,
            LEAST(
                width_bucket(
                    v.value::float8,
                    d.min_value::float8,
                    d.max_value::float8,
                    $2
                ),
                $2
            )::int AS bucket,
            COUNT(*) AS count
        FROM rooms__poll_dimensions d
        JOIN rooms__votes v ON v.dimension_id = d.id AND v.poll_id = $1
        WHERE d.poll_id = $1
        GROUP BY d.id, d.name, d.min_value, d.max_value, bucket
        ORDER BY d.sort_order ASC, bucket ASC
        ",
    )
    .bind(poll_id)
    .bind(NUM_BUCKETS)
    .fetch_all(pool)
    .await?;

    // Fetch dimension ordering (needed to include dimensions with zero votes)
    let dim_rows = sqlx::query_as::<_, (uuid::Uuid, String, f32, f32)>(
        r"
        SELECT id, name, min_value, max_value
        FROM rooms__poll_dimensions
        WHERE poll_id = $1
        ORDER BY sort_order ASC
        ",
    )
    .bind(poll_id)
    .fetch_all(pool)
    .await?;

    // Group distribution rows by dimension
    let mut row_map: std::collections::HashMap<uuid::Uuid, Vec<DistributionRow>> =
        std::collections::HashMap::new();
    for row in rows {
        row_map.entry(row.dimension_id).or_default().push(row);
    }

    // Build result — ensure all 10 buckets present for every dimension
    let result = dim_rows
        .into_iter()
        .map(|(id, name, min_value, max_value)| {
            let dim_rows = row_map.remove(&id).unwrap_or_default();
            let bucket_map: std::collections::HashMap<i32, i64> =
                dim_rows.into_iter().map(|r| (r.bucket, r.count)).collect();

            let buckets = (1..=NUM_BUCKETS)
                .map(|b| BucketCount {
                    bucket: b,
                    count: bucket_map.get(&b).copied().unwrap_or(0),
                })
                .collect();

            DimensionDistribution {
                dimension_id: id,
                dimension_name: name,
                min_value,
                max_value,
                buckets,
            }
        })
        .collect();

    Ok(result)
}
