//! Endorsement helpers for test setup — bypass the action queue to insert edges directly.

use sqlx::PgPool;
use uuid::Uuid;

/// Insert an active endorsement directly into the DB (bypass the action queue for test setup).
pub async fn insert_endorsement(pool: &PgPool, endorser: Uuid, subject: Uuid, weight: f32) {
    sqlx::query(
        "INSERT INTO reputation__endorsements (endorser_id, subject_id, topic, weight)
         VALUES ($1, $2, 'trust', $3)",
    )
    .bind(endorser)
    .bind(subject)
    .bind(weight)
    .execute(pool)
    .await
    .unwrap();
}

/// Insert a revoked endorsement (revoked_at set to now).
pub async fn insert_revoked_endorsement(pool: &PgPool, endorser: Uuid, subject: Uuid, weight: f32) {
    sqlx::query(
        "INSERT INTO reputation__endorsements (endorser_id, subject_id, topic, weight, revoked_at)
         VALUES ($1, $2, 'trust', $3, NOW())",
    )
    .bind(endorser)
    .bind(subject)
    .bind(weight)
    .execute(pool)
    .await
    .unwrap();
}
