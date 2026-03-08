#![allow(clippy::missing_const_for_fn)]

use sqlx::PgPool;
use uuid::Uuid;

use super::{InfluenceRecord, TrustRepoError};

pub(super) fn get_or_create_influence(
    _pool: &PgPool,
    _user_id: Uuid,
) -> Result<InfluenceRecord, TrustRepoError> {
    Err(TrustRepoError::Database(sqlx::Error::RowNotFound))
}

pub(super) fn update_influence(
    _pool: &PgPool,
    _user_id: Uuid,
    _staked_delta: f32,
    _spent_delta: f32,
) -> Result<InfluenceRecord, TrustRepoError> {
    Err(TrustRepoError::Database(sqlx::Error::RowNotFound))
}
