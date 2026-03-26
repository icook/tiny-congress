use sqlx::PgPool;
use uuid::Uuid;

use super::{ScoreSnapshot, TrustRepoError};

/// Shared UPDATE SET clause for both upsert branches.
///
/// The global score upsert (`context_user_id IS NULL`) and the context-specific
/// upsert use different ON CONFLICT targets (two separate partial indexes) but an
/// identical SET clause. Extracting it here ensures that a new column added to
/// `trust__score_snapshots` is automatically reflected in both paths.
const UPSERT_SET: &str = "SET trust_distance = EXCLUDED.trust_distance, \
                           path_diversity = EXCLUDED.path_diversity, \
                           eigenvector_centrality = EXCLUDED.eigenvector_centrality, \
                           computed_at = now()";

pub(super) async fn upsert_score(
    pool: &PgPool,
    user_id: Uuid,
    context_user_id: Option<Uuid>,
    distance: Option<f32>,
    diversity: Option<i32>,
    centrality: Option<f32>,
) -> Result<(), TrustRepoError> {
    if let Some(ctx_id) = context_user_id {
        sqlx::query(&format!(
            "INSERT INTO trust__score_snapshots \
             (user_id, context_user_id, trust_distance, path_diversity, eigenvector_centrality) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (user_id, context_user_id) DO UPDATE {UPSERT_SET}"
        ))
        .bind(user_id)
        .bind(ctx_id)
        .bind(distance)
        .bind(diversity)
        .bind(centrality)
        .execute(pool)
        .await?;
    } else {
        sqlx::query(&format!(
            "INSERT INTO trust__score_snapshots \
             (user_id, context_user_id, trust_distance, path_diversity, eigenvector_centrality) \
             VALUES ($1, NULL, $2, $3, $4) \
             ON CONFLICT (user_id) WHERE context_user_id IS NULL DO UPDATE {UPSERT_SET}"
        ))
        .bind(user_id)
        .bind(distance)
        .bind(diversity)
        .bind(centrality)
        .execute(pool)
        .await?;
    }

    Ok(())
}

pub(super) async fn get_score(
    pool: &PgPool,
    user_id: Uuid,
    context_user_id: Option<Uuid>,
) -> Result<Option<ScoreSnapshot>, TrustRepoError> {
    let record = sqlx::query_as::<_, ScoreSnapshot>(
        "SELECT user_id, context_user_id, trust_distance, path_diversity, \
         eigenvector_centrality, computed_at \
         FROM trust__score_snapshots \
         WHERE user_id = $1 AND context_user_id IS NOT DISTINCT FROM $2",
    )
    .bind(user_id)
    .bind(context_user_id)
    .fetch_optional(pool)
    .await?;

    Ok(record)
}

pub(super) async fn get_all_scores(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Vec<ScoreSnapshot>, TrustRepoError> {
    let records = sqlx::query_as::<_, ScoreSnapshot>(
        "SELECT user_id, context_user_id, trust_distance, path_diversity, \
         eigenvector_centrality, computed_at \
         FROM trust__score_snapshots \
         WHERE user_id = $1 \
         ORDER BY computed_at DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    Ok(records)
}
