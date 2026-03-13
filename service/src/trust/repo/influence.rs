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
