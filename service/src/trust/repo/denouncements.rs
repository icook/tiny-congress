#![allow(clippy::missing_const_for_fn)]

use sqlx::PgPool;
use uuid::Uuid;

use super::{DenouncementRecord, TrustRepoError};

pub(super) fn create_denouncement(
    _pool: &PgPool,
    _accuser_id: Uuid,
    _target_id: Uuid,
    _reason: &str,
    _influence_spent: f32,
) -> Result<DenouncementRecord, TrustRepoError> {
    Err(TrustRepoError::Database(sqlx::Error::RowNotFound))
}

pub(super) fn list_denouncements_against(
    _pool: &PgPool,
    _target_id: Uuid,
) -> Result<Vec<DenouncementRecord>, TrustRepoError> {
    Err(TrustRepoError::Database(sqlx::Error::RowNotFound))
}

pub(super) fn count_active_denouncements_by(
    _pool: &PgPool,
    _accuser_id: Uuid,
) -> Result<i64, TrustRepoError> {
    Err(TrustRepoError::Database(sqlx::Error::RowNotFound))
}
