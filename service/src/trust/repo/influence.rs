use sqlx::PgPool;
use uuid::Uuid;

use super::{InfluenceRecord, TrustRepoError};

pub(super) async fn get_or_create_influence(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<InfluenceRecord, TrustRepoError> {
    // Insert a default row if one does not already exist, then SELECT it back.
    sqlx::query(
        "INSERT INTO trust__user_influence (user_id) VALUES ($1) ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(user_id)
    .execute(pool)
    .await?;

    let record = sqlx::query_as::<_, InfluenceRecord>(
        "SELECT user_id, total_influence, staked_influence, spent_influence, updated_at \
         FROM trust__user_influence WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;

    Ok(record)
}

pub(super) async fn update_influence(
    pool: &PgPool,
    user_id: Uuid,
    staked_delta: f32,
    spent_delta: f32,
) -> Result<InfluenceRecord, TrustRepoError> {
    // Update atomically, enforcing that staked and spent are non-negative and
    // that available influence (total - staked - spent) never goes below zero.
    let maybe = sqlx::query_as::<_, InfluenceRecord>(
        "UPDATE trust__user_influence \
         SET staked_influence = staked_influence + $2, \
             spent_influence  = spent_influence  + $3, \
             updated_at       = now() \
         WHERE user_id = $1 \
           AND (staked_influence + $2) >= 0 \
           AND (spent_influence  + $3) >= 0 \
           AND (total_influence - (staked_influence + $2) - (spent_influence + $3)) >= 0 \
         RETURNING user_id, total_influence, staked_influence, spent_influence, updated_at",
    )
    .bind(user_id)
    .bind(staked_delta)
    .bind(spent_delta)
    .fetch_optional(pool)
    .await?;

    maybe.ok_or(TrustRepoError::InsufficientBudget)
}
