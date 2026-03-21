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
/// Maximum character count of a denouncement reason (matches the user-facing "500 characters" limit).
pub const DENOUNCEMENT_REASON_MAX_LEN: usize = 500;

/// Returns `true` if `reason` is a valid denouncement reason: non-empty, not whitespace-only,
/// and within the max length.
///
/// Uses Unicode scalar value (character) count, not byte count, so that multi-byte scripts
/// (e.g. Chinese, Arabic) are measured the same way as the user-facing error message.
///
/// Whitespace-only strings are rejected (consistent with device name and username validation
/// elsewhere in the codebase). Per the "reject, don't sanitize" principle, trimming is not
/// applied — callers must submit meaningful content.
pub(crate) fn is_valid_reason(reason: &str) -> bool {
    !reason.trim().is_empty() && reason.chars().count() <= DENOUNCEMENT_REASON_MAX_LEN
}

/// The canonical set of trust action types, shared between the service (write) and
/// the worker (read/parse).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionType {
    Endorse,
    Revoke,
    Denounce,
}

impl ActionType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Endorse => "endorse",
            Self::Revoke => "revoke",
            Self::Denounce => "denounce",
        }
    }

    /// Parse an action type from its string representation, returning `None`
    /// for unrecognised values.
    #[must_use]
    pub fn from_str_opt(s: &str) -> Option<Self> {
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
    fn is_valid_weight_accepts_minimum_positive() {
        assert!(is_valid_endorsement_weight(f32::MIN_POSITIVE));
    }

    #[test]
    fn is_valid_weight_accepts_one() {
        assert!(is_valid_endorsement_weight(1.0));
    }

    #[test]
    fn is_valid_weight_accepts_midrange() {
        assert!(is_valid_endorsement_weight(0.5));
    }

    #[test]
    fn is_valid_weight_rejects_zero() {
        assert!(!is_valid_endorsement_weight(0.0));
    }

    #[test]
    fn is_valid_weight_rejects_negative() {
        assert!(!is_valid_endorsement_weight(-0.1));
    }

    #[test]
    fn is_valid_weight_rejects_above_one() {
        assert!(!is_valid_endorsement_weight(1.1));
    }

    #[test]
    fn is_valid_weight_rejects_nan() {
        assert!(!is_valid_endorsement_weight(f32::NAN));
    }

    #[test]
    fn is_valid_weight_rejects_positive_infinity() {
        assert!(!is_valid_endorsement_weight(f32::INFINITY));
    }

    #[test]
    fn is_valid_weight_rejects_negative_infinity() {
        assert!(!is_valid_endorsement_weight(f32::NEG_INFINITY));
    }

    #[test]
    fn is_valid_reason_accepts_nonempty_within_limit() {
        assert!(is_valid_reason("valid reason"));
    }

    #[test]
    fn is_valid_reason_rejects_empty() {
        assert!(!is_valid_reason(""));
    }

    #[test]
    fn is_valid_reason_rejects_whitespace_only() {
        assert!(!is_valid_reason(" "));
        assert!(!is_valid_reason("   "));
        assert!(!is_valid_reason("\t"));
        assert!(!is_valid_reason("\n"));
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
    fn is_valid_reason_accepts_500_multibyte_chars() {
        // Each '中' is 3 bytes; 500 of them is 1500 bytes but only 500 characters.
        // The old `.len()` check would have rejected this; `.chars().count()` accepts it.
        let reason = "中".repeat(DENOUNCEMENT_REASON_MAX_LEN);
        assert!(
            is_valid_reason(&reason),
            "500 multi-byte characters should be accepted"
        );
    }

    #[test]
    fn is_valid_reason_rejects_501_multibyte_chars() {
        let reason = "中".repeat(DENOUNCEMENT_REASON_MAX_LEN + 1);
        assert!(
            !is_valid_reason(&reason),
            "501 multi-byte characters should be rejected"
        );
    }

    #[test]
    fn action_type_round_trips_all_variants() {
        // Every variant's as_str() must be accepted by from_str_opt().
        // This catches the case where a new variant is added to ActionType
        // but from_str_opt() is not updated to match.
        for variant in [
            ActionType::Endorse,
            ActionType::Revoke,
            ActionType::Denounce,
        ] {
            assert_eq!(
                ActionType::from_str_opt(variant.as_str()),
                Some(variant),
                "{variant:?}.as_str() did not round-trip through from_str_opt"
            );
        }
    }

    #[test]
    fn action_type_from_str_opt_rejects_unknown() {
        assert_eq!(ActionType::from_str_opt("unknown"), None);
        assert_eq!(ActionType::from_str_opt(""), None);
    }

    #[test]
    fn action_type_from_str_opt_is_case_sensitive() {
        assert_eq!(ActionType::from_str_opt("Endorse"), None);
        assert_eq!(ActionType::from_str_opt("REVOKE"), None);
        assert_eq!(ActionType::from_str_opt("Denounce"), None);
    }

    // ─── DefaultTrustService early-exit validation tests ─────────────────────
    //
    // These tests cover the validation guards in endorse/revoke/denounce that
    // fire BEFORE any repository call. Stub repos panic on every method so that
    // a test failure instantly surfaces if the guard is missing or out of order.

    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};
    use uuid::Uuid;

    use crate::reputation::repo::{
        CreatedEndorsement, EndorsementRecord, EndorsementRepoError, ExternalIdentityRecord,
        ExternalIdentityRepoError, ReputationRepo,
    };
    use crate::trust::repo::{
        ActionRecord, DenouncementRecord, DenouncementWithUsername, InfluenceRecord, InviteRecord,
        ScoreSnapshot, TrustRepo, TrustRepoError,
    };
    use crate::trust::weight::{DeliveryMethod, RelationshipDepth};

    struct PanicTrustRepo;
    struct PanicReputationRepo;

    #[async_trait]
    impl TrustRepo for PanicTrustRepo {
        async fn get_or_create_influence(
            &self,
            _: Uuid,
        ) -> Result<InfluenceRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn enqueue_action(
            &self,
            _: Uuid,
            _: ActionType,
            _: &serde_json::Value,
        ) -> Result<ActionRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn count_daily_actions(&self, _: Uuid) -> Result<i64, TrustRepoError> {
            unimplemented!()
        }
        async fn get_action(&self, _: Uuid) -> Result<ActionRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn complete_action(&self, _: Uuid) -> Result<(), TrustRepoError> {
            unimplemented!()
        }
        async fn fail_action(&self, _: Uuid, _: &str) -> Result<(), TrustRepoError> {
            unimplemented!()
        }
        async fn create_denouncement(
            &self,
            _: Uuid,
            _: Uuid,
            _: &str,
        ) -> Result<DenouncementRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn create_denouncement_and_revoke_endorsement(
            &self,
            _: Uuid,
            _: Uuid,
            _: &str,
        ) -> Result<DenouncementRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn list_denouncements_against(
            &self,
            _: Uuid,
        ) -> Result<Vec<DenouncementRecord>, TrustRepoError> {
            unimplemented!()
        }
        async fn list_denouncements_by(
            &self,
            _: Uuid,
        ) -> Result<Vec<DenouncementRecord>, TrustRepoError> {
            unimplemented!()
        }
        async fn list_denouncements_by_with_username(
            &self,
            _: Uuid,
        ) -> Result<Vec<DenouncementWithUsername>, TrustRepoError> {
            unimplemented!()
        }
        async fn count_total_denouncements_by(&self, _: Uuid) -> Result<i64, TrustRepoError> {
            unimplemented!()
        }
        async fn has_active_denouncement(&self, _: Uuid, _: Uuid) -> Result<bool, TrustRepoError> {
            unimplemented!()
        }
        async fn create_invite(
            &self,
            _: Uuid,
            _: &[u8],
            _: DeliveryMethod,
            _: Option<RelationshipDepth>,
            _: f32,
            _: &serde_json::Value,
            _: chrono::DateTime<chrono::Utc>,
        ) -> Result<InviteRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn get_invite(&self, _: Uuid) -> Result<InviteRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn accept_invite(&self, _: Uuid, _: Uuid) -> Result<InviteRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn list_invites_by_endorser(
            &self,
            _: Uuid,
        ) -> Result<Vec<InviteRecord>, TrustRepoError> {
            unimplemented!()
        }
        async fn upsert_score(
            &self,
            _: Uuid,
            _: Option<Uuid>,
            _: Option<f32>,
            _: Option<i32>,
            _: Option<f32>,
        ) -> Result<(), TrustRepoError> {
            unimplemented!()
        }
        async fn get_score(
            &self,
            _: Uuid,
            _: Option<Uuid>,
        ) -> Result<Option<ScoreSnapshot>, TrustRepoError> {
            unimplemented!()
        }
        async fn get_all_scores(&self, _: Uuid) -> Result<Vec<ScoreSnapshot>, TrustRepoError> {
            unimplemented!()
        }
        async fn has_identity_endorsement(
            &self,
            _: Uuid,
            _: &[Uuid],
            _: &str,
        ) -> Result<bool, TrustRepoError> {
            unimplemented!()
        }
    }

    #[async_trait]
    impl ReputationRepo for PanicReputationRepo {
        async fn create_endorsement(
            &self,
            _: Uuid,
            _: &str,
            _: Option<Uuid>,
            _: Option<&serde_json::Value>,
            _: f32,
            _: Option<&serde_json::Value>,
            _: bool,
        ) -> Result<CreatedEndorsement, EndorsementRepoError> {
            unimplemented!()
        }
        async fn count_all_active_trust_endorsements_by(
            &self,
            _: Uuid,
        ) -> Result<i64, EndorsementRepoError> {
            unimplemented!()
        }
        async fn has_endorsement(&self, _: Uuid, _: &str) -> Result<bool, EndorsementRepoError> {
            unimplemented!()
        }
        async fn list_endorsements_by_subject(
            &self,
            _: Uuid,
        ) -> Result<Vec<EndorsementRecord>, EndorsementRepoError> {
            unimplemented!()
        }
        async fn revoke_endorsement(
            &self,
            _: Uuid,
            _: Uuid,
            _: &str,
        ) -> Result<(), EndorsementRepoError> {
            unimplemented!()
        }
        async fn count_active_trust_endorsements_by(
            &self,
            _: Uuid,
        ) -> Result<i64, EndorsementRepoError> {
            unimplemented!()
        }
        async fn link_external_identity(
            &self,
            _: Uuid,
            _: &str,
            _: &str,
        ) -> Result<ExternalIdentityRecord, ExternalIdentityRepoError> {
            unimplemented!()
        }
        async fn get_external_identity_by_provider(
            &self,
            _: &str,
            _: &str,
        ) -> Result<ExternalIdentityRecord, ExternalIdentityRepoError> {
            unimplemented!()
        }
    }

    fn make_service() -> DefaultTrustService {
        DefaultTrustService::new(Arc::new(PanicTrustRepo), Arc::new(PanicReputationRepo))
    }

    /// Stub [`TrustRepo`] that returns "under quota" and "has active denouncement" —
    /// used to test the [`TrustServiceError::DenouncementConflict`] guard in
    /// [`DefaultTrustService::endorse`] without reaching the reputation repo.
    struct BelowQuotaWithActiveDenouncement;

    #[async_trait]
    impl TrustRepo for BelowQuotaWithActiveDenouncement {
        async fn count_daily_actions(&self, _: Uuid) -> Result<i64, TrustRepoError> {
            Ok(0)
        }
        async fn has_active_denouncement(&self, _: Uuid, _: Uuid) -> Result<bool, TrustRepoError> {
            Ok(true)
        }
        async fn get_or_create_influence(
            &self,
            _: Uuid,
        ) -> Result<InfluenceRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn enqueue_action(
            &self,
            _: Uuid,
            _: ActionType,
            _: &serde_json::Value,
        ) -> Result<ActionRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn get_action(&self, _: Uuid) -> Result<ActionRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn complete_action(&self, _: Uuid) -> Result<(), TrustRepoError> {
            unimplemented!()
        }
        async fn fail_action(&self, _: Uuid, _: &str) -> Result<(), TrustRepoError> {
            unimplemented!()
        }
        async fn create_denouncement(
            &self,
            _: Uuid,
            _: Uuid,
            _: &str,
        ) -> Result<DenouncementRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn create_denouncement_and_revoke_endorsement(
            &self,
            _: Uuid,
            _: Uuid,
            _: &str,
        ) -> Result<DenouncementRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn list_denouncements_against(
            &self,
            _: Uuid,
        ) -> Result<Vec<DenouncementRecord>, TrustRepoError> {
            unimplemented!()
        }
        async fn list_denouncements_by(
            &self,
            _: Uuid,
        ) -> Result<Vec<DenouncementRecord>, TrustRepoError> {
            unimplemented!()
        }
        async fn list_denouncements_by_with_username(
            &self,
            _: Uuid,
        ) -> Result<Vec<DenouncementWithUsername>, TrustRepoError> {
            unimplemented!()
        }
        async fn count_total_denouncements_by(&self, _: Uuid) -> Result<i64, TrustRepoError> {
            unimplemented!()
        }
        async fn create_invite(
            &self,
            _: Uuid,
            _: &[u8],
            _: DeliveryMethod,
            _: Option<RelationshipDepth>,
            _: f32,
            _: &serde_json::Value,
            _: chrono::DateTime<chrono::Utc>,
        ) -> Result<InviteRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn get_invite(&self, _: Uuid) -> Result<InviteRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn accept_invite(&self, _: Uuid, _: Uuid) -> Result<InviteRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn list_invites_by_endorser(
            &self,
            _: Uuid,
        ) -> Result<Vec<InviteRecord>, TrustRepoError> {
            unimplemented!()
        }
        async fn upsert_score(
            &self,
            _: Uuid,
            _: Option<Uuid>,
            _: Option<f32>,
            _: Option<i32>,
            _: Option<f32>,
        ) -> Result<(), TrustRepoError> {
            unimplemented!()
        }
        async fn get_score(
            &self,
            _: Uuid,
            _: Option<Uuid>,
        ) -> Result<Option<ScoreSnapshot>, TrustRepoError> {
            unimplemented!()
        }
        async fn get_all_scores(&self, _: Uuid) -> Result<Vec<ScoreSnapshot>, TrustRepoError> {
            unimplemented!()
        }
        async fn has_identity_endorsement(
            &self,
            _: Uuid,
            _: &[Uuid],
            _: &str,
        ) -> Result<bool, TrustRepoError> {
            unimplemented!()
        }
    }

    #[tokio::test]
    async fn endorse_returns_denouncement_conflict_when_endorser_has_denounced_subject() {
        // ADR-024: endorsement and denouncement are mutually exclusive.
        // Calling endorse() when the endorser already has an active denouncement
        // against the subject must return DenouncementConflict without enqueueing.
        let svc = DefaultTrustService::new(
            Arc::new(BelowQuotaWithActiveDenouncement),
            Arc::new(PanicReputationRepo),
        );
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let err = svc.endorse(a, b, 0.5, None).await.unwrap_err();
        assert!(
            matches!(err, TrustServiceError::DenouncementConflict),
            "expected DenouncementConflict, got: {err}"
        );
    }

    #[tokio::test]
    async fn endorse_returns_self_action_when_endorser_equals_subject() {
        let id = Uuid::new_v4();
        let err = make_service().endorse(id, id, 0.5, None).await.unwrap_err();
        assert!(matches!(err, TrustServiceError::SelfAction));
    }

    #[tokio::test]
    async fn endorse_returns_invalid_weight_for_zero() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let err = make_service().endorse(a, b, 0.0, None).await.unwrap_err();
        assert!(matches!(err, TrustServiceError::InvalidWeight));
    }

    #[tokio::test]
    async fn endorse_returns_invalid_weight_for_nan() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let err = make_service()
            .endorse(a, b, f32::NAN, None)
            .await
            .unwrap_err();
        assert!(matches!(err, TrustServiceError::InvalidWeight));
    }

    #[tokio::test]
    async fn revoke_endorsement_returns_self_action_when_ids_match() {
        let id = Uuid::new_v4();
        let err = make_service().revoke_endorsement(id, id).await.unwrap_err();
        assert!(matches!(err, TrustServiceError::SelfAction));
    }

    #[tokio::test]
    async fn denounce_returns_self_action_when_accuser_equals_target() {
        let id = Uuid::new_v4();
        let err = make_service().denounce(id, id, "reason").await.unwrap_err();
        assert!(matches!(err, TrustServiceError::SelfAction));
    }

    #[tokio::test]
    async fn denounce_returns_invalid_reason_for_empty_string() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let err = make_service().denounce(a, b, "").await.unwrap_err();
        assert!(matches!(err, TrustServiceError::InvalidReason { .. }));
    }

    #[tokio::test]
    async fn denounce_returns_invalid_reason_for_whitespace_only() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let err = make_service().denounce(a, b, "   ").await.unwrap_err();
        assert!(matches!(err, TrustServiceError::InvalidReason { .. }));
    }

    #[tokio::test]
    async fn denounce_returns_invalid_reason_when_reason_exceeds_max_len() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let long_reason = "x".repeat(DENOUNCEMENT_REASON_MAX_LEN + 1);
        let err = make_service()
            .denounce(a, b, &long_reason)
            .await
            .unwrap_err();
        assert!(matches!(err, TrustServiceError::InvalidReason { .. }));
    }

    /// Stub [`TrustRepo`] that simulates a user who has no active denouncement against
    /// the target and is under their daily quota, but has already consumed all
    /// permanent denouncement slots. Used to test the
    /// [`TrustServiceError::DenouncementSlotsExhausted`] guard in
    /// [`DefaultTrustService::denounce`] without reaching `enqueue_action`.
    struct AtDenouncementSlotLimitRepo;

    #[async_trait]
    impl TrustRepo for AtDenouncementSlotLimitRepo {
        async fn count_daily_actions(&self, _: Uuid) -> Result<i64, TrustRepoError> {
            Ok(0)
        }
        async fn has_active_denouncement(&self, _: Uuid, _: Uuid) -> Result<bool, TrustRepoError> {
            Ok(false)
        }
        async fn count_total_denouncements_by(&self, _: Uuid) -> Result<i64, TrustRepoError> {
            Ok(i64::from(DENOUNCEMENT_SLOT_LIMIT))
        }
        async fn get_or_create_influence(
            &self,
            _: Uuid,
        ) -> Result<InfluenceRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn enqueue_action(
            &self,
            _: Uuid,
            _: ActionType,
            _: &serde_json::Value,
        ) -> Result<ActionRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn get_action(&self, _: Uuid) -> Result<ActionRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn complete_action(&self, _: Uuid) -> Result<(), TrustRepoError> {
            unimplemented!()
        }
        async fn fail_action(&self, _: Uuid, _: &str) -> Result<(), TrustRepoError> {
            unimplemented!()
        }
        async fn create_denouncement(
            &self,
            _: Uuid,
            _: Uuid,
            _: &str,
        ) -> Result<DenouncementRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn create_denouncement_and_revoke_endorsement(
            &self,
            _: Uuid,
            _: Uuid,
            _: &str,
        ) -> Result<DenouncementRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn list_denouncements_against(
            &self,
            _: Uuid,
        ) -> Result<Vec<DenouncementRecord>, TrustRepoError> {
            unimplemented!()
        }
        async fn list_denouncements_by(
            &self,
            _: Uuid,
        ) -> Result<Vec<DenouncementRecord>, TrustRepoError> {
            unimplemented!()
        }
        async fn list_denouncements_by_with_username(
            &self,
            _: Uuid,
        ) -> Result<Vec<DenouncementWithUsername>, TrustRepoError> {
            unimplemented!()
        }
        async fn create_invite(
            &self,
            _: Uuid,
            _: &[u8],
            _: DeliveryMethod,
            _: Option<RelationshipDepth>,
            _: f32,
            _: &serde_json::Value,
            _: chrono::DateTime<chrono::Utc>,
        ) -> Result<InviteRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn get_invite(&self, _: Uuid) -> Result<InviteRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn accept_invite(&self, _: Uuid, _: Uuid) -> Result<InviteRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn list_invites_by_endorser(
            &self,
            _: Uuid,
        ) -> Result<Vec<InviteRecord>, TrustRepoError> {
            unimplemented!()
        }
        async fn upsert_score(
            &self,
            _: Uuid,
            _: Option<Uuid>,
            _: Option<f32>,
            _: Option<i32>,
            _: Option<f32>,
        ) -> Result<(), TrustRepoError> {
            unimplemented!()
        }
        async fn get_score(
            &self,
            _: Uuid,
            _: Option<Uuid>,
        ) -> Result<Option<ScoreSnapshot>, TrustRepoError> {
            unimplemented!()
        }
        async fn get_all_scores(&self, _: Uuid) -> Result<Vec<ScoreSnapshot>, TrustRepoError> {
            unimplemented!()
        }
        async fn has_identity_endorsement(
            &self,
            _: Uuid,
            _: &[Uuid],
            _: &str,
        ) -> Result<bool, TrustRepoError> {
            unimplemented!()
        }
    }

    #[tokio::test]
    async fn denounce_returns_slots_exhausted_when_at_denouncement_limit() {
        // The permanent denouncement budget (d=2) is full. The guard must fire
        // before enqueue_action is called — AtDenouncementSlotLimitRepo panics
        // on enqueue_action, so a missing guard would cause the test to panic.
        let svc = DefaultTrustService::new(
            Arc::new(AtDenouncementSlotLimitRepo),
            Arc::new(PanicReputationRepo),
        );
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let err = svc.denounce(a, b, "valid reason").await.unwrap_err();
        assert!(
            matches!(
                err,
                TrustServiceError::DenouncementSlotsExhausted { max }
                    if max == DENOUNCEMENT_SLOT_LIMIT
            ),
            "expected DenouncementSlotsExhausted with max={DENOUNCEMENT_SLOT_LIMIT}, got: {err}"
        );
    }

    /// Stub [`TrustRepo`] that returns no active denouncement but reports the daily
    /// action quota as exhausted — used to test the [`TrustServiceError::QuotaExceeded`]
    /// guard in [`DefaultTrustService::denounce`] without reaching
    /// `count_total_denouncements_by`.
    struct AtDailyQuotaForDenounceRepo;

    #[async_trait]
    impl TrustRepo for AtDailyQuotaForDenounceRepo {
        async fn count_daily_actions(&self, _: Uuid) -> Result<i64, TrustRepoError> {
            Ok(DAILY_ACTION_QUOTA)
        }
        async fn has_active_denouncement(&self, _: Uuid, _: Uuid) -> Result<bool, TrustRepoError> {
            Ok(false)
        }
        async fn get_or_create_influence(
            &self,
            _: Uuid,
        ) -> Result<InfluenceRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn enqueue_action(
            &self,
            _: Uuid,
            _: ActionType,
            _: &serde_json::Value,
        ) -> Result<ActionRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn get_action(&self, _: Uuid) -> Result<ActionRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn complete_action(&self, _: Uuid) -> Result<(), TrustRepoError> {
            unimplemented!()
        }
        async fn fail_action(&self, _: Uuid, _: &str) -> Result<(), TrustRepoError> {
            unimplemented!()
        }
        async fn create_denouncement(
            &self,
            _: Uuid,
            _: Uuid,
            _: &str,
        ) -> Result<DenouncementRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn create_denouncement_and_revoke_endorsement(
            &self,
            _: Uuid,
            _: Uuid,
            _: &str,
        ) -> Result<DenouncementRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn list_denouncements_against(
            &self,
            _: Uuid,
        ) -> Result<Vec<DenouncementRecord>, TrustRepoError> {
            unimplemented!()
        }
        async fn list_denouncements_by(
            &self,
            _: Uuid,
        ) -> Result<Vec<DenouncementRecord>, TrustRepoError> {
            unimplemented!()
        }
        async fn list_denouncements_by_with_username(
            &self,
            _: Uuid,
        ) -> Result<Vec<DenouncementWithUsername>, TrustRepoError> {
            unimplemented!()
        }
        async fn count_total_denouncements_by(&self, _: Uuid) -> Result<i64, TrustRepoError> {
            unimplemented!()
        }
        async fn create_invite(
            &self,
            _: Uuid,
            _: &[u8],
            _: DeliveryMethod,
            _: Option<RelationshipDepth>,
            _: f32,
            _: &serde_json::Value,
            _: chrono::DateTime<chrono::Utc>,
        ) -> Result<InviteRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn get_invite(&self, _: Uuid) -> Result<InviteRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn accept_invite(&self, _: Uuid, _: Uuid) -> Result<InviteRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn list_invites_by_endorser(
            &self,
            _: Uuid,
        ) -> Result<Vec<InviteRecord>, TrustRepoError> {
            unimplemented!()
        }
        async fn upsert_score(
            &self,
            _: Uuid,
            _: Option<Uuid>,
            _: Option<f32>,
            _: Option<i32>,
            _: Option<f32>,
        ) -> Result<(), TrustRepoError> {
            unimplemented!()
        }
        async fn get_score(
            &self,
            _: Uuid,
            _: Option<Uuid>,
        ) -> Result<Option<ScoreSnapshot>, TrustRepoError> {
            unimplemented!()
        }
        async fn get_all_scores(&self, _: Uuid) -> Result<Vec<ScoreSnapshot>, TrustRepoError> {
            unimplemented!()
        }
        async fn has_identity_endorsement(
            &self,
            _: Uuid,
            _: &[Uuid],
            _: &str,
        ) -> Result<bool, TrustRepoError> {
            unimplemented!()
        }
    }

    #[tokio::test]
    async fn denounce_returns_quota_exceeded_when_daily_limit_reached() {
        // The daily action quota is exhausted. The guard must fire before
        // count_total_denouncements_by is reached — AtDailyQuotaForDenounceRepo
        // panics on count_total_denouncements_by, so a missing guard causes the
        // test to panic rather than silently pass.
        let svc = DefaultTrustService::new(
            Arc::new(AtDailyQuotaForDenounceRepo),
            Arc::new(PanicReputationRepo),
        );
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let err = svc.denounce(a, b, "valid reason").await.unwrap_err();
        assert!(
            matches!(err, TrustServiceError::QuotaExceeded),
            "expected QuotaExceeded, got: {err}"
        );
    }

    // ─── in_slot computation tests ───────────────────────────────────────────
    //
    // Every test above stops before `has_endorsement` because the guard it is
    // testing fires first.  These tests let all guards pass and verify the
    // `in_slot` flag written into the enqueued payload:
    //   - verifier accounts always receive `in_slot = true`, regardless of how
    //     many active endorsements they have (the exemption).
    //   - non-verifier accounts at the slot limit receive `in_slot = false` but
    //     the action is still queued — slots full is NOT an error.

    /// A [`TrustRepo`] stub that passes the two pre-enqueue guards (quota and
    /// denouncement checks) and captures the `enqueue_action` payload for
    /// inspection.
    struct CapturingEnqueueRepo {
        captured: Arc<Mutex<Option<serde_json::Value>>>,
    }

    #[async_trait]
    impl TrustRepo for CapturingEnqueueRepo {
        async fn count_daily_actions(&self, _: Uuid) -> Result<i64, TrustRepoError> {
            Ok(0) // below quota
        }
        async fn has_active_denouncement(&self, _: Uuid, _: Uuid) -> Result<bool, TrustRepoError> {
            Ok(false) // no conflict
        }
        async fn enqueue_action(
            &self,
            actor_id: Uuid,
            action_type: ActionType,
            payload: &serde_json::Value,
        ) -> Result<ActionRecord, TrustRepoError> {
            *self.captured.lock().unwrap() = Some(payload.clone());
            Ok(ActionRecord {
                id: Uuid::new_v4(),
                actor_id,
                action_type: action_type.as_str().to_string(),
                payload: payload.clone(),
                status: "pending".to_string(),
                quota_date: chrono::Utc::now().date_naive(),
                error_message: None,
                created_at: chrono::Utc::now(),
                processed_at: None,
            })
        }
        async fn get_or_create_influence(
            &self,
            _: Uuid,
        ) -> Result<InfluenceRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn get_action(&self, _: Uuid) -> Result<ActionRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn complete_action(&self, _: Uuid) -> Result<(), TrustRepoError> {
            unimplemented!()
        }
        async fn fail_action(&self, _: Uuid, _: &str) -> Result<(), TrustRepoError> {
            unimplemented!()
        }
        async fn create_denouncement(
            &self,
            _: Uuid,
            _: Uuid,
            _: &str,
        ) -> Result<DenouncementRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn create_denouncement_and_revoke_endorsement(
            &self,
            _: Uuid,
            _: Uuid,
            _: &str,
        ) -> Result<DenouncementRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn list_denouncements_against(
            &self,
            _: Uuid,
        ) -> Result<Vec<DenouncementRecord>, TrustRepoError> {
            unimplemented!()
        }
        async fn list_denouncements_by(
            &self,
            _: Uuid,
        ) -> Result<Vec<DenouncementRecord>, TrustRepoError> {
            unimplemented!()
        }
        async fn list_denouncements_by_with_username(
            &self,
            _: Uuid,
        ) -> Result<Vec<DenouncementWithUsername>, TrustRepoError> {
            unimplemented!()
        }
        async fn count_total_denouncements_by(&self, _: Uuid) -> Result<i64, TrustRepoError> {
            unimplemented!()
        }
        async fn create_invite(
            &self,
            _: Uuid,
            _: &[u8],
            _: DeliveryMethod,
            _: Option<RelationshipDepth>,
            _: f32,
            _: &serde_json::Value,
            _: chrono::DateTime<chrono::Utc>,
        ) -> Result<InviteRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn get_invite(&self, _: Uuid) -> Result<InviteRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn accept_invite(&self, _: Uuid, _: Uuid) -> Result<InviteRecord, TrustRepoError> {
            unimplemented!()
        }
        async fn list_invites_by_endorser(
            &self,
            _: Uuid,
        ) -> Result<Vec<InviteRecord>, TrustRepoError> {
            unimplemented!()
        }
        async fn upsert_score(
            &self,
            _: Uuid,
            _: Option<Uuid>,
            _: Option<f32>,
            _: Option<i32>,
            _: Option<f32>,
        ) -> Result<(), TrustRepoError> {
            unimplemented!()
        }
        async fn get_score(
            &self,
            _: Uuid,
            _: Option<Uuid>,
        ) -> Result<Option<ScoreSnapshot>, TrustRepoError> {
            unimplemented!()
        }
        async fn get_all_scores(&self, _: Uuid) -> Result<Vec<ScoreSnapshot>, TrustRepoError> {
            unimplemented!()
        }
        async fn has_identity_endorsement(
            &self,
            _: Uuid,
            _: &[Uuid],
            _: &str,
        ) -> Result<bool, TrustRepoError> {
            unimplemented!()
        }
    }

    /// A [`ReputationRepo`] stub with configurable verifier and active-endorsement
    /// values, used to drive the `in_slot` computation in [`DefaultTrustService::endorse`].
    struct StubReputationRepo {
        is_verifier: bool,
        active_endorsements: i64,
    }

    #[async_trait]
    impl ReputationRepo for StubReputationRepo {
        async fn has_endorsement(&self, _: Uuid, _: &str) -> Result<bool, EndorsementRepoError> {
            Ok(self.is_verifier)
        }
        async fn count_active_trust_endorsements_by(
            &self,
            _: Uuid,
        ) -> Result<i64, EndorsementRepoError> {
            Ok(self.active_endorsements)
        }
        async fn create_endorsement(
            &self,
            _: Uuid,
            _: &str,
            _: Option<Uuid>,
            _: Option<&serde_json::Value>,
            _: f32,
            _: Option<&serde_json::Value>,
            _: bool,
        ) -> Result<CreatedEndorsement, EndorsementRepoError> {
            unimplemented!()
        }
        async fn count_all_active_trust_endorsements_by(
            &self,
            _: Uuid,
        ) -> Result<i64, EndorsementRepoError> {
            unimplemented!()
        }
        async fn list_endorsements_by_subject(
            &self,
            _: Uuid,
        ) -> Result<Vec<EndorsementRecord>, EndorsementRepoError> {
            unimplemented!()
        }
        async fn revoke_endorsement(
            &self,
            _: Uuid,
            _: Uuid,
            _: &str,
        ) -> Result<(), EndorsementRepoError> {
            unimplemented!()
        }
        async fn link_external_identity(
            &self,
            _: Uuid,
            _: &str,
            _: &str,
        ) -> Result<ExternalIdentityRecord, ExternalIdentityRepoError> {
            unimplemented!()
        }
        async fn get_external_identity_by_provider(
            &self,
            _: &str,
            _: &str,
        ) -> Result<ExternalIdentityRecord, ExternalIdentityRepoError> {
            unimplemented!()
        }
    }

    #[tokio::test]
    async fn endorse_queues_with_in_slot_true_for_verifier() {
        // Verifier accounts are exempt from the k=3 endorsement slot limit.
        // Even with active_endorsements == ENDORSEMENT_SLOT_LIMIT, the queued
        // payload must carry `in_slot = true`.
        let captured = Arc::new(Mutex::new(None));
        let svc = DefaultTrustService::new(
            Arc::new(CapturingEnqueueRepo {
                captured: captured.clone(),
            }),
            Arc::new(StubReputationRepo {
                is_verifier: true,
                active_endorsements: i64::from(ENDORSEMENT_SLOT_LIMIT), // slots "full"
            }),
        );
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        svc.endorse(a, b, 0.5, None).await.unwrap();
        let payload = captured.lock().unwrap().clone().unwrap();
        assert_eq!(
            payload["in_slot"],
            serde_json::Value::Bool(true),
            "verifier endorsement must always be in-slot regardless of active count"
        );
    }

    #[tokio::test]
    async fn endorse_queues_with_in_slot_false_when_non_verifier_slots_full() {
        // When a non-verifier has exhausted their k=3 endorsement slots, the
        // action is still queued (no error) but with `in_slot = false`.  The
        // endorsement does not count toward the slot quota.
        let captured = Arc::new(Mutex::new(None));
        let svc = DefaultTrustService::new(
            Arc::new(CapturingEnqueueRepo {
                captured: captured.clone(),
            }),
            Arc::new(StubReputationRepo {
                is_verifier: false,
                active_endorsements: i64::from(ENDORSEMENT_SLOT_LIMIT), // at slot limit
            }),
        );
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        // Must succeed — full slots do NOT block the endorsement.
        svc.endorse(a, b, 0.5, None).await.unwrap();
        let payload = captured.lock().unwrap().clone().unwrap();
        assert_eq!(
            payload["in_slot"],
            serde_json::Value::Bool(false),
            "non-verifier with full slots must be queued as out-of-slot, not rejected"
        );
    }

    #[tokio::test]
    async fn endorse_queues_with_in_slot_true_when_non_verifier_has_available_slots() {
        // A non-verifier with fewer active endorsements than the slot limit must
        // receive `in_slot = true` in the queued payload. This tests the `<`
        // boundary in `active_count < endorsement_slots` — the complement of the
        // slot-full test above.
        let captured = Arc::new(Mutex::new(None));
        let svc = DefaultTrustService::new(
            Arc::new(CapturingEnqueueRepo {
                captured: captured.clone(),
            }),
            Arc::new(StubReputationRepo {
                is_verifier: false,
                active_endorsements: i64::from(ENDORSEMENT_SLOT_LIMIT) - 1, // one slot free
            }),
        );
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        svc.endorse(a, b, 0.5, None).await.unwrap();
        let payload = captured.lock().unwrap().clone().unwrap();
        assert_eq!(
            payload["in_slot"],
            serde_json::Value::Bool(true),
            "non-verifier with available slots must be queued as in-slot"
        );
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
            .enqueue_action(endorser_id, ActionType::Endorse, &payload)
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
            .enqueue_action(endorser_id, ActionType::Revoke, &payload)
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
            .enqueue_action(accuser_id, ActionType::Denounce, &payload)
            .await?;

        Ok(())
    }
}
