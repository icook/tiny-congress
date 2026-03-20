//! Trust service layer — orchestrates influence, action queue, and score computation.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use uuid::Uuid;

use crate::reputation::repo::ReputationRepo;
use crate::trust::repo::{TrustRepo, TrustRepoError};
/// Demo endorsement slot limit per user (k=3).
pub const ENDORSEMENT_SLOT_LIMIT: u32 = 3;
/// Permanent denouncement budget per user (d=2, ADR-020).
pub const DENOUNCEMENT_SLOT_LIMIT: u32 = 2;
/// Max trust actions per user per day (resets at midnight UTC).
pub const DAILY_ACTION_QUOTA: i64 = 5;
/// Maximum byte length of a denouncement reason (matches migration CHECK constraint).
pub const DENOUNCEMENT_REASON_MAX_LEN: usize = 500;

/// Returns `true` if `reason` is a valid denouncement reason: non-empty and within the max length.
pub(crate) const fn is_valid_reason(reason: &str) -> bool {
    !reason.is_empty() && reason.len() <= DENOUNCEMENT_REASON_MAX_LEN
}

/// The canonical set of trust action types, shared between the service (write) and
/// the worker (read/parse).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActionType {
    Endorse,
    Revoke,
    Denounce,
}

impl ActionType {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Endorse => "endorse",
            Self::Revoke => "revoke",
            Self::Denounce => "denounce",
        }
    }

    /// Parse an action type from its string representation, returning `None`
    /// for unrecognised values.
    pub(crate) fn from_str_opt(s: &str) -> Option<Self> {
        match s {
            "endorse" => Some(Self::Endorse),
            "revoke" => Some(Self::Revoke),
            "denounce" => Some(Self::Denounce),
            _ => None,
        }
    }
}

/// Returns `true` if `weight` is a valid endorsement weight: finite and in (0.0, 1.0].
#[allow(clippy::missing_const_for_fn)]
pub(crate) fn is_valid_endorsement_weight(weight: f32) -> bool {
    weight.is_finite() && weight > 0.0 && weight <= 1.0
}

/// Errors returned by [`TrustService`] operations.
#[derive(Debug, thiserror::Error)]
pub enum TrustServiceError {
    #[error("denouncement slots exhausted (max {max})")]
    DenouncementSlotsExhausted { max: u32 },

    #[error("daily action quota exceeded")]
    QuotaExceeded,

    #[error("cannot target yourself")]
    SelfAction,

    #[error("cannot endorse a user you have denounced")]
    DenouncementConflict,

    #[error("already denounced this user")]
    AlreadyDenounced,

    #[error("weight must be in range (0.0, 1.0]")]
    InvalidWeight,

    #[error("reason must be between 1 and {max} characters")]
    InvalidReason { max: usize },

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
    max_denouncement_slots: u32, // d=2
    /// Max actions per day (resets at midnight UTC)
    daily_quota: i64, // 5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_valid_reason_accepts_nonempty_within_limit() {
        assert!(is_valid_reason("valid reason"));
    }

    #[test]
    fn is_valid_reason_rejects_empty() {
        assert!(!is_valid_reason(""));
    }

    #[test]
    fn is_valid_reason_accepts_exactly_max_len() {
        let reason = "a".repeat(DENOUNCEMENT_REASON_MAX_LEN);
        assert!(is_valid_reason(&reason));
    }

    #[test]
    fn is_valid_reason_rejects_over_max_len() {
        let reason = "a".repeat(DENOUNCEMENT_REASON_MAX_LEN + 1);
        assert!(!is_valid_reason(&reason));
    }

    #[test]
    fn is_valid_endorsement_weight_accepts_midrange() {
        assert!(is_valid_endorsement_weight(0.5));
    }

    #[test]
    fn is_valid_endorsement_weight_accepts_exactly_one() {
        // upper bound is inclusive: weight=1.0 is valid
        assert!(is_valid_endorsement_weight(1.0));
    }

    #[test]
    fn is_valid_endorsement_weight_rejects_zero() {
        // lower bound is exclusive: weight=0.0 is invalid
        assert!(!is_valid_endorsement_weight(0.0));
    }

    #[test]
    fn is_valid_endorsement_weight_rejects_above_one() {
        assert!(!is_valid_endorsement_weight(1.1));
    }

    #[test]
    fn is_valid_endorsement_weight_rejects_negative() {
        assert!(!is_valid_endorsement_weight(-0.1));
    }

    #[test]
    fn is_valid_endorsement_weight_rejects_nan() {
        assert!(!is_valid_endorsement_weight(f32::NAN));
    }

    #[test]
    fn is_valid_endorsement_weight_rejects_infinity() {
        assert!(!is_valid_endorsement_weight(f32::INFINITY));
    }
}

impl DefaultTrustService {
    /// Create a new `DefaultTrustService` with default slot and quota limits.
    #[must_use]
    pub fn new(trust_repo: Arc<dyn TrustRepo>, reputation_repo: Arc<dyn ReputationRepo>) -> Self {
        Self {
            trust_repo,
            reputation_repo,
            endorsement_slots: ENDORSEMENT_SLOT_LIMIT,
            max_denouncement_slots: DENOUNCEMENT_SLOT_LIMIT,
            daily_quota: DAILY_ACTION_QUOTA,
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

        if !is_valid_endorsement_weight(weight) {
            return Err(TrustServiceError::InvalidWeight);
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
            .enqueue_action(endorser_id, ActionType::Endorse.as_str(), &payload)
            .await?;

        Ok(())
    }

    async fn revoke_endorsement(
        &self,
        endorser_id: Uuid,
        subject_id: Uuid,
    ) -> Result<(), TrustServiceError> {
        if endorser_id == subject_id {
            return Err(TrustServiceError::SelfAction);
        }

        let daily_count = self.trust_repo.count_daily_actions(endorser_id).await?;
        if daily_count >= self.daily_quota {
            return Err(TrustServiceError::QuotaExceeded);
        }

        let payload = json!({ "subject_id": subject_id });
        self.trust_repo
            .enqueue_action(endorser_id, ActionType::Revoke.as_str(), &payload)
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

        if !is_valid_reason(reason) {
            return Err(TrustServiceError::InvalidReason {
                max: DENOUNCEMENT_REASON_MAX_LEN,
            });
        }

        // Cannot file a denouncement against someone already denounced. This mirrors
        // the DenouncementConflict check on the endorse path and prevents the user
        // from wasting their daily quota on an action the worker will silently reject.
        let already_denounced = self
            .trust_repo
            .has_active_denouncement(accuser_id, target_id)
            .await?;
        if already_denounced {
            return Err(TrustServiceError::AlreadyDenounced);
        }

        let daily_count = self.trust_repo.count_daily_actions(accuser_id).await?;
        if daily_count >= self.daily_quota {
            return Err(TrustServiceError::QuotaExceeded);
        }

        let total_denouncements = self
            .trust_repo
            .count_total_denouncements_by(accuser_id)
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
            .enqueue_action(accuser_id, ActionType::Denounce.as_str(), &payload)
            .await?;

        Ok(())
    }
}
