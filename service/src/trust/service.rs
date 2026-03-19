//! Trust service layer — orchestrates influence, action queue, and score computation.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use uuid::Uuid;

use crate::reputation::repo::ReputationRepo;
use crate::trust::repo::{TrustRepo, TrustRepoError};

/// Errors returned by [`TrustService`] operations.
#[derive(Debug, thiserror::Error)]
pub enum TrustServiceError {
    #[error("endorsement slots exhausted (max {max})")]
    EndorsementSlotsExhausted { max: u32 },

    #[error("denouncement slots exhausted (max {max})")]
    DenouncementSlotsExhausted { max: i32 },

    #[error("daily action quota exceeded")]
    QuotaExceeded,

    #[error("cannot target yourself")]
    SelfAction,

    #[error("cannot endorse a user you have denounced")]
    DenouncementConflict,

    #[error("repository error: {0}")]
    Repo(#[from] TrustRepoError),

    #[error("endorsement repo error: {0}")]
    EndorsementRepo(#[from] crate::reputation::repo::EndorsementRepoError),
}

/// Service trait for trust action orchestration.
#[async_trait]
pub trait TrustService: Send + Sync {
    /// Endorse another user. Validates slots and daily quota before enqueueing.
    async fn endorse(
        &self,
        endorser_id: Uuid,
        subject_id: Uuid,
        weight: f32,
        attestation: Option<serde_json::Value>,
    ) -> Result<(), TrustServiceError>;

    /// Revoke an endorsement. Frees the endorsement slot.
    async fn revoke_endorsement(
        &self,
        endorser_id: Uuid,
        subject_id: Uuid,
    ) -> Result<(), TrustServiceError>;

    /// File a denouncement against another user.
    async fn denounce(
        &self,
        accuser_id: Uuid,
        target_id: Uuid,
        reason: &str,
    ) -> Result<(), TrustServiceError>;
}

/// Default implementation of [`TrustService`] backed by a [`TrustRepo`].
pub struct DefaultTrustService {
    trust_repo: Arc<dyn TrustRepo>,
    reputation_repo: Arc<dyn ReputationRepo>,
    /// Max active endorsement slots per user (k=3 demo, k=5 production)
    endorsement_slots: u32,
    /// Max active denouncements per user
    max_denouncement_slots: i32, // d=2
    /// Max actions per day (resets at midnight UTC)
    daily_quota: i64, // 5
}

impl DefaultTrustService {
    /// Create a new `DefaultTrustService` with default slot and quota limits.
    #[must_use]
    pub fn new(trust_repo: Arc<dyn TrustRepo>, reputation_repo: Arc<dyn ReputationRepo>) -> Self {
        Self {
            trust_repo,
            reputation_repo,
            endorsement_slots: 3,
            max_denouncement_slots: 2,
            daily_quota: 5,
        }
    }
}

#[async_trait]
impl TrustService for DefaultTrustService {
    async fn endorse(
        &self,
        endorser_id: Uuid,
        subject_id: Uuid,
        weight: f32,
        attestation: Option<serde_json::Value>,
    ) -> Result<(), TrustServiceError> {
        if endorser_id == subject_id {
            return Err(TrustServiceError::SelfAction);
        }

        let daily_count = self.trust_repo.count_daily_actions(endorser_id).await?;
        if daily_count >= self.daily_quota {
            return Err(TrustServiceError::QuotaExceeded);
        }

        // Denouncement and endorsement are mutually exclusive: cannot endorse
        // someone you have denounced (ADR-024).
        let already_denounced = self
            .trust_repo
            .has_active_denouncement(endorser_id, subject_id)
            .await?;
        if already_denounced {
            return Err(TrustServiceError::DenouncementConflict);
        }

        // Verifier accounts are exempt from endorsement slot limits
        let is_verifier = self
            .reputation_repo
            .has_endorsement(endorser_id, "authorized_verifier")
            .await?;

        let in_slot = if is_verifier {
            true
        } else {
            let active_count = self
                .reputation_repo
                .count_active_trust_endorsements_by(endorser_id)
                .await?;
            active_count < i64::from(self.endorsement_slots)
        };

        let payload = json!({
            "subject_id": subject_id,
            "weight": weight,
            "attestation": attestation,
            "in_slot": in_slot,
        });
        self.trust_repo
            .enqueue_action(endorser_id, "endorse", &payload)
            .await?;

        Ok(())
    }

    async fn revoke_endorsement(
        &self,
        endorser_id: Uuid,
        subject_id: Uuid,
    ) -> Result<(), TrustServiceError> {
        let daily_count = self.trust_repo.count_daily_actions(endorser_id).await?;
        if daily_count >= self.daily_quota {
            return Err(TrustServiceError::QuotaExceeded);
        }

        let payload = json!({ "subject_id": subject_id });
        self.trust_repo
            .enqueue_action(endorser_id, "revoke", &payload)
            .await?;

        Ok(())
    }

    async fn denounce(
        &self,
        accuser_id: Uuid,
        target_id: Uuid,
        reason: &str,
    ) -> Result<(), TrustServiceError> {
        if accuser_id == target_id {
            return Err(TrustServiceError::SelfAction);
        }

        let daily_count = self.trust_repo.count_daily_actions(accuser_id).await?;
        if daily_count >= self.daily_quota {
            return Err(TrustServiceError::QuotaExceeded);
        }

        let total_denouncements = self
            .trust_repo
            .count_active_denouncements_by(accuser_id)
            .await?;
        if total_denouncements >= i64::from(self.max_denouncement_slots) {
            return Err(TrustServiceError::DenouncementSlotsExhausted {
                max: self.max_denouncement_slots,
            });
        }

        let payload = json!({
            "target_id": target_id,
            "reason": reason,
        });
        self.trust_repo
            .enqueue_action(accuser_id, "denounce", &payload)
            .await?;

        Ok(())
    }
}
