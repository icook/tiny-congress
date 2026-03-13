//! Repository layer for trust persistence

pub mod action_queue;
pub mod denouncements;
pub mod influence;
pub mod invites;
pub mod scores;

use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

/// Error type for trust repository operations.
#[derive(Debug)]
pub enum TrustRepoError {
    NotFound,
    Duplicate,
    Database(sqlx::Error),
}

impl std::fmt::Display for TrustRepoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "not found"),
            Self::Duplicate => write!(f, "duplicate"),
            Self::Database(e) => write!(f, "database error: {e}"),
        }
    }
}

impl std::error::Error for TrustRepoError {}

impl From<sqlx::Error> for TrustRepoError {
    fn from(e: sqlx::Error) -> Self {
        Self::Database(e)
    }
}

/// Influence balance for a user.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct InfluenceRecord {
    pub user_id: Uuid,
    pub total_influence: f32,
    pub staked_influence: f32,
    pub spent_influence: f32,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// A queued trust action awaiting batch processing.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ActionRecord {
    pub id: Uuid,
    pub actor_id: Uuid,
    pub action_type: String,
    pub payload: serde_json::Value,
    pub status: String,
    pub quota_date: chrono::NaiveDate,
    pub error_message: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub processed_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// A denouncement filed by one user against another.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DenouncementRecord {
    pub id: Uuid,
    pub accuser_id: Uuid,
    pub target_id: Uuid,
    pub reason: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// An invite issued by an endorser.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct InviteRecord {
    pub id: Uuid,
    pub endorser_id: Uuid,
    pub envelope: Vec<u8>,
    pub delivery_method: String,
    pub attestation: serde_json::Value,
    pub accepted_by: Option<Uuid>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub accepted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// A cached trust score snapshot for a user.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ScoreSnapshot {
    pub user_id: Uuid,
    pub context_user_id: Option<Uuid>,
    pub trust_distance: Option<f32>,
    pub path_diversity: Option<i32>,
    pub eigenvector_centrality: Option<f32>,
    pub computed_at: chrono::DateTime<chrono::Utc>,
}

/// Consolidated repository trait for trust persistence.
#[allow(clippy::too_many_arguments)]
#[async_trait]
pub trait TrustRepo: Send + Sync {
    // Influence operations

    async fn get_or_create_influence(
        &self,
        user_id: Uuid,
    ) -> Result<InfluenceRecord, TrustRepoError>;

    // Action queue operations

    async fn enqueue_action(
        &self,
        actor_id: Uuid,
        action_type: &str,
        payload: &serde_json::Value,
    ) -> Result<ActionRecord, TrustRepoError>;

    async fn count_daily_actions(&self, actor_id: Uuid) -> Result<i64, TrustRepoError>;

    async fn claim_pending_actions(&self, limit: i64) -> Result<Vec<ActionRecord>, TrustRepoError>;

    async fn complete_action(&self, action_id: Uuid) -> Result<(), TrustRepoError>;

    async fn fail_action(&self, action_id: Uuid, error: &str) -> Result<(), TrustRepoError>;

    // Denouncement operations

    async fn create_denouncement(
        &self,
        accuser_id: Uuid,
        target_id: Uuid,
        reason: &str,
    ) -> Result<DenouncementRecord, TrustRepoError>;

    async fn list_denouncements_against(
        &self,
        target_id: Uuid,
    ) -> Result<Vec<DenouncementRecord>, TrustRepoError>;

    async fn count_active_denouncements_by(&self, accuser_id: Uuid) -> Result<i64, TrustRepoError>;

    // Invite operations

    #[allow(clippy::too_many_arguments)]
    async fn create_invite(
        &self,
        endorser_id: Uuid,
        envelope: &[u8],
        delivery_method: &str,
        attestation: &serde_json::Value,
        expires_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<InviteRecord, TrustRepoError>;

    async fn get_invite(&self, invite_id: Uuid) -> Result<InviteRecord, TrustRepoError>;

    async fn accept_invite(
        &self,
        invite_id: Uuid,
        accepted_by: Uuid,
    ) -> Result<InviteRecord, TrustRepoError>;

    async fn list_invites_by_endorser(
        &self,
        endorser_id: Uuid,
    ) -> Result<Vec<InviteRecord>, TrustRepoError>;

    // Score snapshot operations

    #[allow(clippy::too_many_arguments)]
    async fn upsert_score(
        &self,
        user_id: Uuid,
        context_user_id: Option<Uuid>,
        distance: Option<f32>,
        diversity: Option<i32>,
        centrality: Option<f32>,
    ) -> Result<(), TrustRepoError>;

    async fn get_score(
        &self,
        user_id: Uuid,
        context_user_id: Option<Uuid>,
    ) -> Result<Option<ScoreSnapshot>, TrustRepoError>;

    async fn get_all_scores(&self, user_id: Uuid) -> Result<Vec<ScoreSnapshot>, TrustRepoError>;
}

/// `PostgreSQL` implementation of [`TrustRepo`].
pub struct PgTrustRepo {
    pool: PgPool,
}

impl PgTrustRepo {
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[allow(clippy::too_many_arguments)]
#[async_trait]
impl TrustRepo for PgTrustRepo {
    async fn get_or_create_influence(
        &self,
        user_id: Uuid,
    ) -> Result<InfluenceRecord, TrustRepoError> {
        influence::get_or_create_influence(&self.pool, user_id).await
    }

    async fn enqueue_action(
        &self,
        actor_id: Uuid,
        action_type: &str,
        payload: &serde_json::Value,
    ) -> Result<ActionRecord, TrustRepoError> {
        action_queue::enqueue_action(&self.pool, actor_id, action_type, payload).await
    }

    async fn count_daily_actions(&self, actor_id: Uuid) -> Result<i64, TrustRepoError> {
        action_queue::count_daily_actions(&self.pool, actor_id).await
    }

    async fn claim_pending_actions(&self, limit: i64) -> Result<Vec<ActionRecord>, TrustRepoError> {
        action_queue::claim_pending_actions(&self.pool, limit).await
    }

    async fn complete_action(&self, action_id: Uuid) -> Result<(), TrustRepoError> {
        action_queue::complete_action(&self.pool, action_id).await
    }

    async fn fail_action(&self, action_id: Uuid, error: &str) -> Result<(), TrustRepoError> {
        action_queue::fail_action(&self.pool, action_id, error).await
    }

    async fn create_denouncement(
        &self,
        accuser_id: Uuid,
        target_id: Uuid,
        reason: &str,
    ) -> Result<DenouncementRecord, TrustRepoError> {
        denouncements::create_denouncement(&self.pool, accuser_id, target_id, reason).await
    }

    async fn list_denouncements_against(
        &self,
        target_id: Uuid,
    ) -> Result<Vec<DenouncementRecord>, TrustRepoError> {
        denouncements::list_denouncements_against(&self.pool, target_id).await
    }

    async fn count_active_denouncements_by(&self, accuser_id: Uuid) -> Result<i64, TrustRepoError> {
        denouncements::count_active_denouncements_by(&self.pool, accuser_id).await
    }

    async fn create_invite(
        &self,
        endorser_id: Uuid,
        envelope: &[u8],
        delivery_method: &str,
        attestation: &serde_json::Value,
        expires_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<InviteRecord, TrustRepoError> {
        invites::create_invite(
            &self.pool,
            endorser_id,
            envelope,
            delivery_method,
            attestation,
            expires_at,
        )
        .await
    }

    async fn get_invite(&self, invite_id: Uuid) -> Result<InviteRecord, TrustRepoError> {
        invites::get_invite(&self.pool, invite_id).await
    }

    async fn accept_invite(
        &self,
        invite_id: Uuid,
        accepted_by: Uuid,
    ) -> Result<InviteRecord, TrustRepoError> {
        invites::accept_invite(&self.pool, invite_id, accepted_by).await
    }

    async fn list_invites_by_endorser(
        &self,
        endorser_id: Uuid,
    ) -> Result<Vec<InviteRecord>, TrustRepoError> {
        invites::list_invites_by_endorser(&self.pool, endorser_id).await
    }

    async fn upsert_score(
        &self,
        user_id: Uuid,
        context_user_id: Option<Uuid>,
        distance: Option<f32>,
        diversity: Option<i32>,
        centrality: Option<f32>,
    ) -> Result<(), TrustRepoError> {
        scores::upsert_score(
            &self.pool,
            user_id,
            context_user_id,
            distance,
            diversity,
            centrality,
        )
        .await
    }

    async fn get_score(
        &self,
        user_id: Uuid,
        context_user_id: Option<Uuid>,
    ) -> Result<Option<ScoreSnapshot>, TrustRepoError> {
        scores::get_score(&self.pool, user_id, context_user_id).await
    }

    async fn get_all_scores(&self, user_id: Uuid) -> Result<Vec<ScoreSnapshot>, TrustRepoError> {
        scores::get_all_scores(&self.pool, user_id).await
    }
}
