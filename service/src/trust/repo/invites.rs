use sqlx::PgPool;
use uuid::Uuid;

use super::{InviteRecord, TrustRepoError};

pub(super) async fn create_invite(
    pool: &PgPool,
    endorser_id: Uuid,
    envelope: &[u8],
    delivery_method: &str,
    attestation: &serde_json::Value,
    expires_at: chrono::DateTime<chrono::Utc>,
) -> Result<InviteRecord, TrustRepoError> {
    let record = sqlx::query_as::<_, InviteRecord>(
        "INSERT INTO trust__invites \
         (endorser_id, envelope, delivery_method, attestation, expires_at) \
         VALUES ($1, $2, $3, $4, $5) \
         RETURNING *",
    )
    .bind(endorser_id)
    .bind(envelope)
    .bind(delivery_method)
    .bind(attestation)
    .bind(expires_at)
    .fetch_one(pool)
    .await?;

    Ok(record)
}

pub(super) async fn get_invite(
    pool: &PgPool,
    invite_id: Uuid,
) -> Result<InviteRecord, TrustRepoError> {
    sqlx::query_as::<_, InviteRecord>("SELECT * FROM trust__invites WHERE id = $1")
        .bind(invite_id)
        .fetch_optional(pool)
        .await?
        .ok_or(TrustRepoError::NotFound)
}

pub(super) async fn accept_invite(
    pool: &PgPool,
    invite_id: Uuid,
    accepted_by: Uuid,
) -> Result<InviteRecord, TrustRepoError> {
    sqlx::query_as::<_, InviteRecord>(
        "UPDATE trust__invites \
         SET accepted_by = $2, accepted_at = now() \
         WHERE id = $1 \
           AND accepted_by IS NULL \
           AND expires_at > now() \
         RETURNING *",
    )
    .bind(invite_id)
    .bind(accepted_by)
    .fetch_optional(pool)
    .await?
    .ok_or(TrustRepoError::NotFound)
}

pub(super) async fn list_invites_by_endorser(
    pool: &PgPool,
    endorser_id: Uuid,
) -> Result<Vec<InviteRecord>, TrustRepoError> {
    let records = sqlx::query_as::<_, InviteRecord>(
        "SELECT * FROM trust__invites \
         WHERE endorser_id = $1 \
         ORDER BY created_at DESC",
    )
    .bind(endorser_id)
    .fetch_all(pool)
    .await?;

    Ok(records)
}
