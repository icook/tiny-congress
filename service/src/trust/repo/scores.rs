#![allow(clippy::missing_const_for_fn)]

use sqlx::PgPool;
use uuid::Uuid;

use super::{ScoreSnapshot, TrustRepoError};

pub(super) fn upsert_score(
    _pool: &PgPool,
    _user_id: Uuid,
    _context_user_id: Option<Uuid>,
    _distance: Option<f32>,
    _diversity: Option<i32>,
    _centrality: Option<f32>,
) -> Result<(), TrustRepoError> {
    Err(TrustRepoError::Database(sqlx::Error::RowNotFound))
}

pub(super) fn get_score(
    _pool: &PgPool,
    _user_id: Uuid,
    _context_user_id: Option<Uuid>,
) -> Result<Option<ScoreSnapshot>, TrustRepoError> {
    Err(TrustRepoError::Database(sqlx::Error::RowNotFound))
}

pub(super) fn get_all_scores(
    _pool: &PgPool,
    _user_id: Uuid,
) -> Result<Vec<ScoreSnapshot>, TrustRepoError> {
    Err(TrustRepoError::Database(sqlx::Error::RowNotFound))
}
