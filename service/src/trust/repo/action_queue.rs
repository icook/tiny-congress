#![allow(clippy::missing_const_for_fn)]

use sqlx::PgPool;
use uuid::Uuid;

use super::{ActionRecord, TrustRepoError};

pub(super) fn enqueue_action(
    _pool: &PgPool,
    _actor_id: Uuid,
    _action_type: &str,
    _payload: &serde_json::Value,
) -> Result<ActionRecord, TrustRepoError> {
    Err(TrustRepoError::Database(sqlx::Error::RowNotFound))
}

pub(super) fn count_daily_actions(_pool: &PgPool, _actor_id: Uuid) -> Result<i64, TrustRepoError> {
    Err(TrustRepoError::Database(sqlx::Error::RowNotFound))
}

pub(super) fn claim_pending_actions(
    _pool: &PgPool,
    _limit: i64,
) -> Result<Vec<ActionRecord>, TrustRepoError> {
    Err(TrustRepoError::Database(sqlx::Error::RowNotFound))
}

pub(super) fn complete_action(_pool: &PgPool, _action_id: Uuid) -> Result<(), TrustRepoError> {
    Err(TrustRepoError::Database(sqlx::Error::RowNotFound))
}

pub(super) fn fail_action(
    _pool: &PgPool,
    _action_id: Uuid,
    _error: &str,
) -> Result<(), TrustRepoError> {
    Err(TrustRepoError::Database(sqlx::Error::RowNotFound))
}
