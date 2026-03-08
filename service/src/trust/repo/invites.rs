#![allow(clippy::missing_const_for_fn)]

use sqlx::PgPool;
use uuid::Uuid;

use super::{InviteRecord, TrustRepoError};

pub(super) fn create_invite(
    _pool: &PgPool,
    _endorser_id: Uuid,
    _envelope: &[u8],
    _delivery_method: &str,
    _attestation: &serde_json::Value,
    _expires_at: chrono::DateTime<chrono::Utc>,
) -> Result<InviteRecord, TrustRepoError> {
    Err(TrustRepoError::Database(sqlx::Error::RowNotFound))
}

pub(super) fn get_invite(_pool: &PgPool, _invite_id: Uuid) -> Result<InviteRecord, TrustRepoError> {
    Err(TrustRepoError::Database(sqlx::Error::RowNotFound))
}

pub(super) fn accept_invite(
    _pool: &PgPool,
    _invite_id: Uuid,
    _accepted_by: Uuid,
) -> Result<InviteRecord, TrustRepoError> {
    Err(TrustRepoError::Database(sqlx::Error::RowNotFound))
}

pub(super) fn list_invites_by_endorser(
    _pool: &PgPool,
    _endorser_id: Uuid,
) -> Result<Vec<InviteRecord>, TrustRepoError> {
    Err(TrustRepoError::Database(sqlx::Error::RowNotFound))
}
