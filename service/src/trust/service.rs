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

    /// Verify the DB-facing string representation for each `ActionType` variant.
    ///
    /// These strings must match the `trust__action_log.action_type` CHECK constraint:
    /// `action_type IN ('endorse', 'revoke', 'denounce')`.
    /// If a variant is renamed or its `as_str()` value changes, the INSERT in
    /// `action_queue::enqueue_action` will fail with a constraint violation at
    /// runtime. This test catches the mismatch before it reaches the database.
    #[test]
    fn action_type_as_str_matches_db_constraint() {
        assert_eq!(ActionType::Endorse.as_str(), "endorse");
        assert_eq!(ActionType::Revoke.as_str(), "revoke");
        assert_eq!(ActionType::Denounce.as_str(), "denounce");
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
    async fn endorse_returns_invalid_weight_for_above_one() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let err = make_service().endorse(a, b, 1.1, None).await.unwrap_err();
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

    // ─── denounce repo-error propagation test ────────────────────────────────

    /// Stub [`TrustRepo`] that fails `has_active_denouncement` with a database
    /// error — used to verify that `denounce` propagates the error rather than
    /// silently swallowing it.
    struct FailingHasActiveDenouncementRepo;

    #[async_trait]
    impl TrustRepo for FailingHasActiveDenouncementRepo {
        async fn has_active_denouncement(&self, _: Uuid, _: Uuid) -> Result<bool, TrustRepoError> {
            Err(TrustRepoError::Database(sqlx::Error::RowNotFound))
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
    async fn denounce_propagates_repo_error_from_has_active_denouncement() {
        // `has_active_denouncement` is the first repo call in `denounce` (after the
        // pure self-action and reason checks). If the database fails there, the
        // service must surface the error rather than silently succeeding or panicking.
        // `FailingHasActiveDenouncementRepo` panics on every other method, so a
        // missing `?` would cause the test to fail loudly.
        let svc = DefaultTrustService::new(
            Arc::new(FailingHasActiveDenouncementRepo),
            Arc::new(PanicReputationRepo),
        );
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let err = svc.denounce(a, b, "valid reason").await.unwrap_err();
        assert!(
            matches!(err, TrustServiceError::Repo(TrustRepoError::Database(_))),
            "expected Repo(Database(...)), got: {err}"
        );
    }

    // ─── endorse repo-error propagation tests ────────────────────────────────

    /// Stub [`TrustRepo`] that fails `count_daily_actions` — used to verify
    /// that `endorse` propagates the error rather than silently swallowing it.
    struct FailingCountDailyActionsForEndorseRepo;

    #[async_trait]
    impl TrustRepo for FailingCountDailyActionsForEndorseRepo {
        async fn count_daily_actions(&self, _: Uuid) -> Result<i64, TrustRepoError> {
            Err(TrustRepoError::Database(sqlx::Error::RowNotFound))
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

    #[tokio::test]
    async fn endorse_propagates_repo_error_from_count_daily_actions() {
        // `count_daily_actions` is the first repo call in `endorse` (after the
        // pure self-action and weight checks). If the database fails there, the
        // service must surface the error rather than silently succeeding or panicking.
        // `FailingCountDailyActionsForEndorseRepo` panics on every other method, so a
        // missing `?` would cause the test to fail loudly.
        let svc = DefaultTrustService::new(
            Arc::new(FailingCountDailyActionsForEndorseRepo),
            Arc::new(PanicReputationRepo),
        );
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let err = svc.endorse(a, b, 0.5, None).await.unwrap_err();
        assert!(
            matches!(err, TrustServiceError::Repo(TrustRepoError::Database(_))),
            "expected Repo(Database(...)), got: {err}"
        );
    }

    /// Stub [`TrustRepo`] that passes `count_daily_actions` but fails
    /// `has_active_denouncement` — used to verify that `endorse` propagates
    /// the error rather than silently swallowing it.
    struct FailingHasActiveDenouncementForEndorseRepo;

    #[async_trait]
    impl TrustRepo for FailingHasActiveDenouncementForEndorseRepo {
        async fn count_daily_actions(&self, _: Uuid) -> Result<i64, TrustRepoError> {
            Ok(0) // below quota — guard passes
        }
        async fn has_active_denouncement(&self, _: Uuid, _: Uuid) -> Result<bool, TrustRepoError> {
            Err(TrustRepoError::Database(sqlx::Error::RowNotFound))
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
    async fn endorse_propagates_repo_error_from_has_active_denouncement() {
        // `has_active_denouncement` is called after `count_daily_actions` passes.
        // If the database fails there, the service must surface the error rather
        // than silently succeeding or panicking.
        // `FailingHasActiveDenouncementForEndorseRepo` panics on every other method,
        // so a missing `?` would cause the test to fail loudly.
        let svc = DefaultTrustService::new(
            Arc::new(FailingHasActiveDenouncementForEndorseRepo),
            Arc::new(PanicReputationRepo),
        );
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let err = svc.endorse(a, b, 0.5, None).await.unwrap_err();
        assert!(
            matches!(err, TrustServiceError::Repo(TrustRepoError::Database(_))),
            "expected Repo(Database(...)), got: {err}"
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
            Ok(0) // below denouncement slot limit — enables denounce happy-path tests
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

    // ─── endorse payload content test ────────────────────────────────────────

    #[tokio::test]
    async fn endorse_enqueues_payload_with_correct_subject_id_and_weight() {
        // Verifies that endorse places the correct subject_id (not the endorser_id)
        // and the verbatim weight into the enqueued payload.  A swap of
        // endorser_id/subject_id would silently pass the in_slot tests above
        // because those tests only inspect the `in_slot` field.
        let captured = Arc::new(Mutex::new(None));
        let svc = DefaultTrustService::new(
            Arc::new(CapturingEnqueueRepo {
                captured: captured.clone(),
            }),
            Arc::new(StubReputationRepo {
                is_verifier: false,
                active_endorsements: 0,
            }),
        );
        let endorser = Uuid::new_v4();
        let subject = Uuid::new_v4();
        let weight = 0.6_f32;
        svc.endorse(endorser, subject, weight, None).await.unwrap();
        let payload = captured.lock().unwrap().clone().unwrap();
        assert_eq!(
            payload["subject_id"],
            serde_json::Value::String(subject.to_string()),
            "payload must carry subject_id, not endorser_id"
        );
        assert!(
            (payload["weight"].as_f64().unwrap() as f32 - weight).abs() < f32::EPSILON,
            "payload must carry the verbatim weight"
        );
    }

    #[tokio::test]
    async fn endorse_enqueues_payload_with_non_null_attestation() {
        // Verifies that a non-null attestation value is forwarded verbatim into
        // the enqueued payload. All other endorse tests pass `attestation = None`;
        // a bug that always serialised `null` would pass every one of them but
        // silently drop attestation data for every real endorsement that carries one.
        let captured = Arc::new(Mutex::new(None));
        let svc = DefaultTrustService::new(
            Arc::new(CapturingEnqueueRepo {
                captured: captured.clone(),
            }),
            Arc::new(StubReputationRepo {
                is_verifier: false,
                active_endorsements: 0,
            }),
        );
        let endorser = Uuid::new_v4();
        let subject = Uuid::new_v4();
        let attestation = serde_json::json!({ "type": "selfie_verified", "confidence": 0.95 });
        svc.endorse(endorser, subject, 0.5, Some(attestation.clone()))
            .await
            .unwrap();
        let payload = captured.lock().unwrap().clone().unwrap();
        assert_eq!(
            payload["attestation"], attestation,
            "payload must carry the verbatim attestation object"
        );
    }

    // ─── denounce payload content test ───────────────────────────────────────

    #[tokio::test]
    async fn denounce_enqueues_payload_with_target_id_and_reason() {
        // Verifies that denounce places the correct target_id (not the accuser_id)
        // and the verbatim reason string into the enqueued payload.  A swap of
        // accuser_id/target_id would silently pass all guard tests above.
        let captured = Arc::new(Mutex::new(None));
        let svc = DefaultTrustService::new(
            Arc::new(CapturingEnqueueRepo {
                captured: captured.clone(),
            }),
            Arc::new(PanicReputationRepo),
        );
        let accuser = Uuid::new_v4();
        let target = Uuid::new_v4();
        let reason = "harmful conduct";
        svc.denounce(accuser, target, reason).await.unwrap();
        let payload = captured.lock().unwrap().clone().unwrap();
        assert_eq!(
            payload["target_id"],
            serde_json::Value::String(target.to_string()),
            "payload must carry target_id, not accuser_id"
        );
        assert_eq!(
            payload["reason"],
            serde_json::Value::String(reason.to_string()),
            "payload must carry the verbatim reason"
        );
    }

    // ─── endorsement repo error propagation ──────────────────────────────────

    /// A [`ReputationRepo`] stub whose `has_endorsement` always returns an error.
    /// Used to verify that `DefaultTrustService::endorse` propagates the error
    /// rather than silently treating the caller as a non-verifier.
    struct ErrorHasEndorsementRepo;

    #[async_trait]
    impl ReputationRepo for ErrorHasEndorsementRepo {
        async fn has_endorsement(&self, _: Uuid, _: &str) -> Result<bool, EndorsementRepoError> {
            Err(EndorsementRepoError::NotFound)
        }
        async fn count_active_trust_endorsements_by(
            &self,
            _: Uuid,
        ) -> Result<i64, EndorsementRepoError> {
            unimplemented!()
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

    // ─── count_active_trust_endorsements_by error propagation ────────────────

    /// A [`ReputationRepo`] stub where `has_endorsement` identifies the caller as
    /// a non-verifier (`Ok(false)`) but `count_active_trust_endorsements_by` fails
    /// with a database error. Used to verify that `DefaultTrustService::endorse`
    /// propagates the error rather than silently treating the active count as 0.
    struct NonVerifierWithFailingActiveCountRepo;

    #[async_trait]
    impl ReputationRepo for NonVerifierWithFailingActiveCountRepo {
        async fn has_endorsement(&self, _: Uuid, _: &str) -> Result<bool, EndorsementRepoError> {
            Ok(false) // non-verifier — proceeds to count_active_trust_endorsements_by
        }
        async fn count_active_trust_endorsements_by(
            &self,
            _: Uuid,
        ) -> Result<i64, EndorsementRepoError> {
            Err(EndorsementRepoError::Database(sqlx::Error::RowNotFound))
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
    async fn endorse_propagates_endorsement_repo_error_when_active_count_check_fails() {
        // When the caller is not a verifier (`has_endorsement` → Ok(false)),
        // `endorse` calls `count_active_trust_endorsements_by` to determine the
        // `in_slot` flag. If that call fails, the error must be propagated as
        // `TrustServiceError::EndorsementRepo` rather than silently treating the
        // count as 0 and proceeding to enqueue. `CapturingEnqueueRepo` panics on
        // `enqueue_action`, so a missing `?` here would surface immediately.
        let captured = Arc::new(Mutex::new(None));
        let svc = DefaultTrustService::new(
            Arc::new(CapturingEnqueueRepo {
                captured: captured.clone(),
            }),
            Arc::new(NonVerifierWithFailingActiveCountRepo),
        );
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let err = svc.endorse(a, b, 0.5, None).await.unwrap_err();
        assert!(
            matches!(err, TrustServiceError::EndorsementRepo(_)),
            "expected EndorsementRepo error when active-count check fails, got: {err}"
        );
        assert!(
            captured.lock().unwrap().is_none(),
            "enqueue_action must not be called when active-count check fails"
        );
    }

    // ─── denounce count_total_denouncements_by error propagation ─────────────

    /// Stub [`TrustRepo`] that passes `has_active_denouncement` and
    /// `count_daily_actions` but fails `count_total_denouncements_by` — used
    /// to verify that `denounce` propagates the error rather than silently
    /// swallowing it or proceeding to `enqueue_action`.
    struct FailingCountTotalDenouncementsByRepo;

    #[async_trait]
    impl TrustRepo for FailingCountTotalDenouncementsByRepo {
        async fn has_active_denouncement(&self, _: Uuid, _: Uuid) -> Result<bool, TrustRepoError> {
            Ok(false) // no existing denouncement → proceed past AlreadyDenounced guard
        }
        async fn count_daily_actions(&self, _: Uuid) -> Result<i64, TrustRepoError> {
            Ok(0) // below quota → proceed past QuotaExceeded guard
        }
        async fn count_total_denouncements_by(&self, _: Uuid) -> Result<i64, TrustRepoError> {
            Err(TrustRepoError::Database(sqlx::Error::RowNotFound))
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
    async fn denounce_propagates_repo_error_from_count_total_denouncements_by() {
        // `count_total_denouncements_by` is called after `has_active_denouncement`
        // and `count_daily_actions` pass. If the database fails there, the service
        // must surface the error rather than silently succeeding or panicking.
        // `FailingCountTotalDenouncementsByRepo` panics on `enqueue_action`, so a
        // missing `?` on the repo call would cause the test to panic loudly.
        let svc = DefaultTrustService::new(
            Arc::new(FailingCountTotalDenouncementsByRepo),
            Arc::new(PanicReputationRepo),
        );
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let err = svc.denounce(a, b, "valid reason").await.unwrap_err();
        assert!(
            matches!(err, TrustServiceError::Repo(TrustRepoError::Database(_))),
            "expected Repo(Database(...)), got: {err}"
        );
    }

    #[tokio::test]
    async fn endorse_propagates_endorsement_repo_error_when_verifier_check_fails() {
        // When `reputation_repo.has_endorsement` (the verifier check) returns an error,
        // `endorse` must propagate it as `TrustServiceError::EndorsementRepo` rather than
        // silently treating the caller as a non-verifier and proceeding to enqueue.
        //
        // Uses `CapturingEnqueueRepo` as the TrustRepo stub so all upstream guards
        // (quota, denouncement conflict) pass, ensuring the path reaches
        // `has_endorsement` before the error is returned.
        let captured = Arc::new(Mutex::new(None));
        let svc = DefaultTrustService::new(
            Arc::new(CapturingEnqueueRepo {
                captured: captured.clone(),
            }),
            Arc::new(ErrorHasEndorsementRepo),
        );
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let err = svc.endorse(a, b, 0.5, None).await.unwrap_err();
        assert!(
            matches!(err, TrustServiceError::EndorsementRepo(_)),
            "expected EndorsementRepo error when verifier check fails, got: {err}"
        );
        // Payload must NOT have been captured — the error fires before enqueue_action.
        assert!(
            captured.lock().unwrap().is_none(),
            "enqueue_action must not be called when verifier check fails"
        );
    }

    // ─── enqueue_action error propagation tests ──────────────────────────────

    /// Stub [`TrustRepo`] that passes all pre-enqueue guards in
    /// [`DefaultTrustService::revoke_endorsement`] (daily quota check) but
    /// returns a database error from `enqueue_action`. Used to verify the
    /// service propagates the error rather than silently succeeding.
    struct FailingEnqueueRepo;

    #[async_trait]
    impl TrustRepo for FailingEnqueueRepo {
        async fn count_daily_actions(&self, _: Uuid) -> Result<i64, TrustRepoError> {
            Ok(0) // below quota — all guards pass
        }
        async fn enqueue_action(
            &self,
            _: Uuid,
            _: ActionType,
            _: &serde_json::Value,
        ) -> Result<ActionRecord, TrustRepoError> {
            Err(TrustRepoError::Database(sqlx::Error::RowNotFound))
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

    /// Stub [`TrustRepo`] that passes all pre-enqueue guards in
    /// [`DefaultTrustService::endorse`] (daily quota check and denouncement
    /// conflict check) but returns a database error from `enqueue_action`.
    /// Used to verify the service propagates the error rather than silently
    /// succeeding.
    struct FailingEnqueueForEndorseRepo;

    #[async_trait]
    impl TrustRepo for FailingEnqueueForEndorseRepo {
        async fn count_daily_actions(&self, _: Uuid) -> Result<i64, TrustRepoError> {
            Ok(0) // below quota — guard passes
        }
        async fn has_active_denouncement(&self, _: Uuid, _: Uuid) -> Result<bool, TrustRepoError> {
            Ok(false) // no conflict — guard passes
        }
        async fn enqueue_action(
            &self,
            _: Uuid,
            _: ActionType,
            _: &serde_json::Value,
        ) -> Result<ActionRecord, TrustRepoError> {
            Err(TrustRepoError::Database(sqlx::Error::RowNotFound))
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

    #[tokio::test]
    async fn endorse_propagates_repo_error_from_enqueue_action() {
        // `enqueue_action` is the last repo call in `endorse` — all guards pass
        // (self-action, weight, daily quota, denouncement conflict, and the
        // verifier/slot checks). If the database fails at the enqueue step, the
        // service must surface the error rather than silently succeeding.
        // `FailingEnqueueForEndorseRepo` panics on all other methods, so a stray
        // call would cause the test to fail loudly.
        let svc = DefaultTrustService::new(
            Arc::new(FailingEnqueueForEndorseRepo),
            Arc::new(StubReputationRepo {
                is_verifier: false,
                active_endorsements: 0,
            }),
        );
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let err = svc.endorse(a, b, 0.5, None).await.unwrap_err();
        assert!(
            matches!(err, TrustServiceError::Repo(TrustRepoError::Database(_))),
            "expected Repo(Database(...)), got: {err}"
        );
    }

    #[tokio::test]
    async fn revoke_endorsement_propagates_repo_error_from_enqueue_action() {
        // `enqueue_action` is the last repo call in `revoke_endorsement` — all
        // guards pass (self-action check and daily quota check). If the database
        // fails at the enqueue step, the service must surface the error rather
        // than silently succeeding. `FailingEnqueueRepo` panics on all other
        // methods, so a stray call would cause the test to fail loudly.
        let svc =
            DefaultTrustService::new(Arc::new(FailingEnqueueRepo), Arc::new(PanicReputationRepo));
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let err = svc.revoke_endorsement(a, b).await.unwrap_err();
        assert!(
            matches!(err, TrustServiceError::Repo(TrustRepoError::Database(_))),
            "expected Repo(Database(...)), got: {err}"
        );
    }

    // ─── denounce enqueue_action error propagation ────────────────────────────

    /// Stub [`TrustRepo`] that passes all pre-enqueue guards in
    /// [`DefaultTrustService::denounce`] (`has_active_denouncement`,
    /// `count_daily_actions`, `count_total_denouncements_by`) but returns a
    /// database error from `enqueue_action`. Used to verify the service
    /// propagates the error rather than silently succeeding.
    struct FailingEnqueueForDenounceRepo;

    #[async_trait]
    impl TrustRepo for FailingEnqueueForDenounceRepo {
        async fn has_active_denouncement(&self, _: Uuid, _: Uuid) -> Result<bool, TrustRepoError> {
            Ok(false) // no existing denouncement — guard passes
        }
        async fn count_daily_actions(&self, _: Uuid) -> Result<i64, TrustRepoError> {
            Ok(0) // below quota — guard passes
        }
        async fn count_total_denouncements_by(&self, _: Uuid) -> Result<i64, TrustRepoError> {
            Ok(0) // below slot limit — guard passes
        }
        async fn enqueue_action(
            &self,
            _: Uuid,
            _: ActionType,
            _: &serde_json::Value,
        ) -> Result<ActionRecord, TrustRepoError> {
            Err(TrustRepoError::Database(sqlx::Error::RowNotFound))
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
    async fn denounce_propagates_repo_error_from_enqueue_action() {
        // `enqueue_action` is the last repo call in `denounce` — all guards pass
        // (self-action, reason validation, `has_active_denouncement`, daily quota,
        // and slot limit checks). If the database fails at the enqueue step, the
        // service must surface the error rather than silently succeeding.
        // `FailingEnqueueForDenounceRepo` panics on all other methods, so a stray
        // call would cause the test to fail loudly.
        let svc = DefaultTrustService::new(
            Arc::new(FailingEnqueueForDenounceRepo),
            Arc::new(PanicReputationRepo),
        );
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let err = svc.denounce(a, b, "valid reason").await.unwrap_err();
        assert!(
            matches!(err, TrustServiceError::Repo(TrustRepoError::Database(_))),
            "expected Repo(Database(...)), got: {err}"
        );
    }

    // ─── action_type pinning tests ────────────────────────────────────────────

    /// A [`TrustRepo`] stub that passes all pre-enqueue guards and captures
    /// the `action_type` argument passed to `enqueue_action`. Used to verify
    /// that each service method uses the correct [`ActionType`] discriminant —
    /// a copy-paste error (e.g. endorse passing `ActionType::Revoke`) would
    /// cause the worker to silently misprocess the queued action.
    struct ActionTypeCapturingRepo {
        captured_type: Arc<Mutex<Option<ActionType>>>,
    }

    #[async_trait]
    impl TrustRepo for ActionTypeCapturingRepo {
        async fn count_daily_actions(&self, _: Uuid) -> Result<i64, TrustRepoError> {
            Ok(0) // below quota — all daily-quota guards pass
        }
        async fn has_active_denouncement(&self, _: Uuid, _: Uuid) -> Result<bool, TrustRepoError> {
            Ok(false) // no conflict or duplicate
        }
        async fn count_total_denouncements_by(&self, _: Uuid) -> Result<i64, TrustRepoError> {
            Ok(0) // below slot limit
        }
        async fn enqueue_action(
            &self,
            actor_id: Uuid,
            action_type: ActionType,
            payload: &serde_json::Value,
        ) -> Result<ActionRecord, TrustRepoError> {
            *self.captured_type.lock().unwrap() = Some(action_type);
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
    async fn endorse_enqueues_action_type_endorse() {
        // Verifies that endorse() passes ActionType::Endorse to enqueue_action.
        // Without this pin, a copy-paste that uses ActionType::Revoke would compile,
        // pass all other tests, and cause the worker to process endorsements as revokes.
        let captured_type = Arc::new(Mutex::new(None));
        let svc = DefaultTrustService::new(
            Arc::new(ActionTypeCapturingRepo {
                captured_type: captured_type.clone(),
            }),
            Arc::new(StubReputationRepo {
                is_verifier: false,
                active_endorsements: 0,
            }),
        );
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        svc.endorse(a, b, 0.5, None).await.unwrap();
        assert_eq!(
            *captured_type.lock().unwrap(),
            Some(ActionType::Endorse),
            "endorse must pass ActionType::Endorse to enqueue_action"
        );
    }

    #[tokio::test]
    async fn revoke_endorsement_enqueues_action_type_revoke() {
        // Verifies that revoke_endorsement() passes ActionType::Revoke to enqueue_action.
        let captured_type = Arc::new(Mutex::new(None));
        let svc = DefaultTrustService::new(
            Arc::new(ActionTypeCapturingRepo {
                captured_type: captured_type.clone(),
            }),
            Arc::new(PanicReputationRepo),
        );
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        svc.revoke_endorsement(a, b).await.unwrap();
        assert_eq!(
            *captured_type.lock().unwrap(),
            Some(ActionType::Revoke),
            "revoke_endorsement must pass ActionType::Revoke to enqueue_action"
        );
    }

    #[tokio::test]
    async fn denounce_enqueues_action_type_denounce() {
        // Verifies that denounce() passes ActionType::Denounce to enqueue_action.
        let captured_type = Arc::new(Mutex::new(None));
        let svc = DefaultTrustService::new(
            Arc::new(ActionTypeCapturingRepo {
                captured_type: captured_type.clone(),
            }),
            Arc::new(PanicReputationRepo),
        );
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        svc.denounce(a, b, "valid reason").await.unwrap();
        assert_eq!(
            *captured_type.lock().unwrap(),
            Some(ActionType::Denounce),
            "denounce must pass ActionType::Denounce to enqueue_action"
        );
    }

    // ─── denounce actor_id correctness ───────────────────────────────────────

    /// Stub [`TrustRepo`] that passes all pre-enqueue guards in
    /// [`DefaultTrustService::denounce`] and captures the `actor_id` argument
    /// passed to `enqueue_action`. Used to verify that `denounce` passes
    /// `accuser_id` (not `target_id`) as the actor.
    struct DenounceActorCapturingRepo {
        captured_actor: Arc<Mutex<Option<Uuid>>>,
    }

    #[async_trait]
    impl TrustRepo for DenounceActorCapturingRepo {
        async fn has_active_denouncement(&self, _: Uuid, _: Uuid) -> Result<bool, TrustRepoError> {
            Ok(false) // no existing denouncement — guard passes
        }
        async fn count_daily_actions(&self, _: Uuid) -> Result<i64, TrustRepoError> {
            Ok(0) // below quota — guard passes
        }
        async fn count_total_denouncements_by(&self, _: Uuid) -> Result<i64, TrustRepoError> {
            Ok(0) // below slot limit — guard passes
        }
        async fn enqueue_action(
            &self,
            actor_id: Uuid,
            action_type: ActionType,
            payload: &serde_json::Value,
        ) -> Result<ActionRecord, TrustRepoError> {
            *self.captured_actor.lock().unwrap() = Some(actor_id);
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

    // ─── revoke_endorsement actor_id correctness ─────────────────────────────

    /// Stub [`TrustRepo`] that passes all pre-enqueue guards in
    /// [`DefaultTrustService::revoke_endorsement`] and captures the `actor_id`
    /// argument passed to `enqueue_action`. Used to verify that
    /// `revoke_endorsement` passes `endorser_id` (not `subject_id`) as the actor.
    struct RevokeActorCapturingRepo {
        captured_actor: Arc<Mutex<Option<Uuid>>>,
    }

    #[async_trait]
    impl TrustRepo for RevokeActorCapturingRepo {
        async fn count_daily_actions(&self, _: Uuid) -> Result<i64, TrustRepoError> {
            Ok(0) // below quota — guard passes
        }
        async fn enqueue_action(
            &self,
            actor_id: Uuid,
            action_type: ActionType,
            payload: &serde_json::Value,
        ) -> Result<ActionRecord, TrustRepoError> {
            *self.captured_actor.lock().unwrap() = Some(actor_id);
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

    #[tokio::test]
    async fn revoke_endorsement_passes_endorser_as_actor_to_enqueue_action() {
        // Verifies that revoke_endorsement() passes endorser_id (not subject_id)
        // as the actor argument to enqueue_action. The payload-content tests only
        // check the payload fields; a bug that swaps endorser_id/subject_id in
        // the actor position would silently pass all other tests but attribute the
        // revocation to the subject's account in the action log.
        let captured_actor = Arc::new(Mutex::new(None::<Uuid>));
        let svc = DefaultTrustService::new(
            Arc::new(RevokeActorCapturingRepo {
                captured_actor: captured_actor.clone(),
            }),
            Arc::new(PanicReputationRepo),
        );
        let endorser = Uuid::new_v4();
        let subject = Uuid::new_v4();
        svc.revoke_endorsement(endorser, subject).await.unwrap();
        assert_eq!(
            *captured_actor.lock().unwrap(),
            Some(endorser),
            "revoke_endorsement must pass endorser_id as actor, not subject_id"
        );
    }

    #[tokio::test]
    async fn denounce_passes_accuser_as_actor_to_enqueue_action() {
        // Verifies that denounce() passes accuser_id (not target_id) as the actor
        // argument to enqueue_action. The payload-content test
        // (denounce_enqueues_payload_with_target_id_and_reason) only checks the
        // payload fields; a bug that swaps accuser_id/target_id in the actor
        // position would silently pass all other tests but attribute the
        // denouncement to the target's account in the action log.
        let captured_actor = Arc::new(Mutex::new(None::<Uuid>));
        let svc = DefaultTrustService::new(
            Arc::new(DenounceActorCapturingRepo {
                captured_actor: captured_actor.clone(),
            }),
            Arc::new(PanicReputationRepo),
        );
        let accuser = Uuid::new_v4();
        let target = Uuid::new_v4();
        svc.denounce(accuser, target, "valid reason").await.unwrap();
        assert_eq!(
            *captured_actor.lock().unwrap(),
            Some(accuser),
            "denounce must pass accuser_id as actor, not target_id"
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
