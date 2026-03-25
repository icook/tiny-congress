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

    /// Single configurable stub for [`ReputationRepo`] used across service-layer tests.
    ///
    /// All methods default to `unimplemented!()`. Set field(s) relevant to your
    /// test; every other method panics if called, so a missing guard in the
    /// code-under-test surfaces as a test panic rather than a silently wrong result.
    ///
    /// When [`ReputationRepo`] gains a new method service tests need to exercise, add
    /// one field here rather than creating a new per-test stub struct.
    #[derive(Default)]
    struct StubReputationRepo {
        /// `Some(v)` → `has_endorsement` returns `Ok(v)`. `None` → `unimplemented!()`.
        endorsement: Option<bool>,
        /// `true` → `has_endorsement` returns a database error.
        endorsement_fails: bool,
        /// If set, captures the `subject_id` argument to `has_endorsement`.
        captured_endorsement_id: Option<Arc<Mutex<Option<Uuid>>>>,
        /// If set, captures the `topic` argument to `has_endorsement`.
        captured_endorsement_topic: Option<Arc<Mutex<Option<String>>>>,
        /// `Some(n)` → `count_active_trust_endorsements_by` returns `Ok(n)`. `None` → `unimplemented!()`.
        active_count: Option<i64>,
        /// `true` → `count_active_trust_endorsements_by` returns a database error.
        active_count_fails: bool,
        /// If set, captures the `endorser_id` argument to `count_active_trust_endorsements_by`.
        captured_active_count_id: Option<Arc<Mutex<Option<Uuid>>>>,
    }

    impl StubReputationRepo {
        fn verifier(mut self, v: bool) -> Self {
            self.endorsement = Some(v);
            self
        }
        fn verifier_error(mut self) -> Self {
            self.endorsement_fails = true;
            self
        }
        fn capture_endorsement_id(mut self, cap: Arc<Mutex<Option<Uuid>>>) -> Self {
            self.captured_endorsement_id = Some(cap);
            self
        }
        fn capture_endorsement_topic(mut self, cap: Arc<Mutex<Option<String>>>) -> Self {
            self.captured_endorsement_topic = Some(cap);
            self
        }
        fn active_count(mut self, n: i64) -> Self {
            self.active_count = Some(n);
            self
        }
        fn active_count_error(mut self) -> Self {
            self.active_count_fails = true;
            self
        }
        fn capture_active_id(mut self, cap: Arc<Mutex<Option<Uuid>>>) -> Self {
            self.captured_active_count_id = Some(cap);
            self
        }
    }

    #[async_trait]
    impl ReputationRepo for StubReputationRepo {
        async fn has_endorsement(
            &self,
            subject_id: Uuid,
            topic: &str,
        ) -> Result<bool, EndorsementRepoError> {
            if let Some(cap) = &self.captured_endorsement_id {
                *cap.lock().unwrap() = Some(subject_id);
            }
            if let Some(cap) = &self.captured_endorsement_topic {
                *cap.lock().unwrap() = Some(topic.to_string());
            }
            if self.endorsement_fails {
                return Err(EndorsementRepoError::NotFound);
            }
            if let Some(v) = self.endorsement {
                return Ok(v);
            }
            unimplemented!()
        }
        async fn count_active_trust_endorsements_by(
            &self,
            endorser_id: Uuid,
        ) -> Result<i64, EndorsementRepoError> {
            if let Some(cap) = &self.captured_active_count_id {
                *cap.lock().unwrap() = Some(endorser_id);
            }
            if self.active_count_fails {
                return Err(EndorsementRepoError::Database(sqlx::Error::RowNotFound));
            }
            if let Some(n) = self.active_count {
                return Ok(n);
            }
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

    /// Single configurable stub for [`TrustRepo`] used across service-layer tests.
    ///
    /// All methods default to `unimplemented!()`. Set field(s) relevant to your
    /// test; every other method panics if called, so a missing guard in the
    /// code-under-test surfaces as a test panic rather than a silently wrong result.
    ///
    /// When [`TrustRepo`] gains a new method service tests need to exercise, add
    /// one field here rather than creating a new per-test stub struct.
    #[derive(Default)]
    struct StubTrustRepo {
        /// `Some(n)` → `count_daily_actions` returns `Ok(n)`. `None` → `unimplemented!()`.
        daily_actions: Option<i64>,
        /// `true` → `count_daily_actions` returns a database error.
        daily_actions_fails: bool,
        /// If set, captures the `actor_id` argument to `count_daily_actions`.
        captured_daily_actor: Option<Arc<Mutex<Option<Uuid>>>>,
        /// `Some(v)` → `has_active_denouncement` returns `Ok(v)`. `None` → `unimplemented!()`.
        active_denouncement: Option<bool>,
        /// `true` → `has_active_denouncement` returns a database error.
        active_denouncement_fails: bool,
        /// If set, captures both `(first, second)` arguments to `has_active_denouncement`.
        captured_active: Option<Arc<Mutex<Option<(Uuid, Uuid)>>>>,
        /// `Some(n)` → `count_total_denouncements_by` returns `Ok(n)`. `None` → `unimplemented!()`.
        total_denouncements: Option<i64>,
        /// `true` → `count_total_denouncements_by` returns a database error.
        total_denouncements_fails: bool,
        /// If set, captures the `actor_id` argument to `count_total_denouncements_by`.
        captured_total_actor: Option<Arc<Mutex<Option<Uuid>>>>,
        /// `true` → `enqueue_action` returns a database error.
        enqueue_fails: bool,
        /// `true` → `enqueue_action` returns a dummy `ActionRecord` (and any capture fields are written).
        /// Automatically true if any `captured_enqueue_*` field is set.
        enqueue_ok: bool,
        /// If set, captures the `payload` argument to `enqueue_action` (implies enqueue succeeds).
        captured_enqueue_payload: Option<Arc<Mutex<Option<serde_json::Value>>>>,
        /// If set, captures the `actor_id` argument to `enqueue_action` (implies enqueue succeeds).
        captured_enqueue_actor: Option<Arc<Mutex<Option<Uuid>>>>,
        /// If set, captures the `action_type` argument to `enqueue_action` (implies enqueue succeeds).
        captured_enqueue_type: Option<Arc<Mutex<Option<ActionType>>>>,
    }

    impl StubTrustRepo {
        fn daily(mut self, n: i64) -> Self {
            self.daily_actions = Some(n);
            self
        }
        fn daily_error(mut self) -> Self {
            self.daily_actions_fails = true;
            self
        }
        fn capture_daily_actor(mut self, cap: Arc<Mutex<Option<Uuid>>>) -> Self {
            self.captured_daily_actor = Some(cap);
            self
        }
        fn active(mut self, v: bool) -> Self {
            self.active_denouncement = Some(v);
            self
        }
        fn active_error(mut self) -> Self {
            self.active_denouncement_fails = true;
            self
        }
        fn capture_active(mut self, cap: Arc<Mutex<Option<(Uuid, Uuid)>>>) -> Self {
            self.captured_active = Some(cap);
            self
        }
        fn total(mut self, n: i64) -> Self {
            self.total_denouncements = Some(n);
            self
        }
        fn total_error(mut self) -> Self {
            self.total_denouncements_fails = true;
            self
        }
        fn capture_total_actor(mut self, cap: Arc<Mutex<Option<Uuid>>>) -> Self {
            self.captured_total_actor = Some(cap);
            self
        }
        fn enqueue_error(mut self) -> Self {
            self.enqueue_fails = true;
            self
        }
        fn enqueue_ok(mut self) -> Self {
            self.enqueue_ok = true;
            self
        }
        fn capture_payload(mut self, cap: Arc<Mutex<Option<serde_json::Value>>>) -> Self {
            self.captured_enqueue_payload = Some(cap);
            self
        }
        fn capture_actor(mut self, cap: Arc<Mutex<Option<Uuid>>>) -> Self {
            self.captured_enqueue_actor = Some(cap);
            self
        }
        fn capture_type(mut self, cap: Arc<Mutex<Option<ActionType>>>) -> Self {
            self.captured_enqueue_type = Some(cap);
            self
        }
    }

    #[async_trait]
    impl TrustRepo for StubTrustRepo {
        async fn count_daily_actions(&self, actor_id: Uuid) -> Result<i64, TrustRepoError> {
            if let Some(cap) = &self.captured_daily_actor {
                *cap.lock().unwrap() = Some(actor_id);
            }
            if self.daily_actions_fails {
                return Err(TrustRepoError::Database(sqlx::Error::RowNotFound));
            }
            if let Some(n) = self.daily_actions {
                return Ok(n);
            }
            unimplemented!()
        }
        async fn has_active_denouncement(
            &self,
            first: Uuid,
            second: Uuid,
        ) -> Result<bool, TrustRepoError> {
            if let Some(cap) = &self.captured_active {
                *cap.lock().unwrap() = Some((first, second));
            }
            if self.active_denouncement_fails {
                return Err(TrustRepoError::Database(sqlx::Error::RowNotFound));
            }
            if let Some(v) = self.active_denouncement {
                return Ok(v);
            }
            unimplemented!()
        }
        async fn count_total_denouncements_by(
            &self,
            actor_id: Uuid,
        ) -> Result<i64, TrustRepoError> {
            if let Some(cap) = &self.captured_total_actor {
                *cap.lock().unwrap() = Some(actor_id);
            }
            if self.total_denouncements_fails {
                return Err(TrustRepoError::Database(sqlx::Error::RowNotFound));
            }
            if let Some(n) = self.total_denouncements {
                return Ok(n);
            }
            unimplemented!()
        }
        async fn enqueue_action(
            &self,
            actor_id: Uuid,
            action_type: ActionType,
            payload: &serde_json::Value,
        ) -> Result<ActionRecord, TrustRepoError> {
            if let Some(cap) = &self.captured_enqueue_payload {
                *cap.lock().unwrap() = Some(payload.clone());
            }
            if let Some(cap) = &self.captured_enqueue_actor {
                *cap.lock().unwrap() = Some(actor_id);
            }
            if let Some(cap) = &self.captured_enqueue_type {
                *cap.lock().unwrap() = Some(action_type);
            }
            if self.enqueue_fails {
                return Err(TrustRepoError::Database(sqlx::Error::RowNotFound));
            }
            let enqueue_enabled = self.enqueue_ok
                || self.captured_enqueue_payload.is_some()
                || self.captured_enqueue_actor.is_some()
                || self.captured_enqueue_type.is_some();
            if enqueue_enabled {
                return Ok(ActionRecord {
                    id: Uuid::new_v4(),
                    actor_id,
                    action_type: action_type.as_str().to_string(),
                    payload: payload.clone(),
                    status: "pending".to_string(),
                    quota_date: chrono::Utc::now().date_naive(),
                    error_message: None,
                    created_at: chrono::Utc::now(),
                    processed_at: None,
                });
            }
            unimplemented!()
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

    fn make_service() -> DefaultTrustService {
        DefaultTrustService::new(
            Arc::new(StubTrustRepo::default()),
            Arc::new(StubReputationRepo::default()),
        )
    }

    #[tokio::test]
    async fn endorse_returns_denouncement_conflict_when_endorser_has_denounced_subject() {
        // ADR-024: endorsement and denouncement are mutually exclusive.
        // Calling endorse() when the endorser already has an active denouncement
        // against the subject must return DenouncementConflict without enqueueing.
        let svc = DefaultTrustService::new(
            Arc::new(StubTrustRepo::default().daily(0).active(true)),
            Arc::new(StubReputationRepo::default()),
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
    async fn endorse_returns_invalid_weight_for_negative() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let err = make_service().endorse(a, b, -0.5, None).await.unwrap_err();
        assert!(matches!(err, TrustServiceError::InvalidWeight));
    }

    #[tokio::test]
    async fn endorse_returns_quota_exceeded_when_daily_limit_reached() {
        // The daily action quota is exhausted. The guard fires before
        // `has_active_denouncement` — StubTrustRepo panics on
        // `has_active_denouncement`, so a missing guard causes the test to panic
        // rather than silently pass.
        let svc = DefaultTrustService::new(
            Arc::new(StubTrustRepo::default().daily(DAILY_ACTION_QUOTA)),
            Arc::new(StubReputationRepo::default()),
        );
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let err = svc.endorse(a, b, 1.0, None).await.unwrap_err();
        assert!(
            matches!(err, TrustServiceError::QuotaExceeded),
            "expected QuotaExceeded, got: {err}"
        );
    }

    #[tokio::test]
    async fn revoke_endorsement_returns_self_action_when_ids_match() {
        let id = Uuid::new_v4();
        let err = make_service().revoke_endorsement(id, id).await.unwrap_err();
        assert!(matches!(err, TrustServiceError::SelfAction));
    }

    #[tokio::test]
    async fn revoke_endorsement_returns_quota_exceeded_when_daily_limit_reached() {
        // The daily action quota is exhausted. The guard fires before
        // `enqueue_action` — StubTrustRepo panics on
        // `enqueue_action`, so a missing guard causes the test to panic
        // rather than silently pass.
        let svc = DefaultTrustService::new(
            Arc::new(StubTrustRepo::default().daily(DAILY_ACTION_QUOTA)),
            Arc::new(StubReputationRepo::default()),
        );
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let err = svc.revoke_endorsement(a, b).await.unwrap_err();
        assert!(
            matches!(err, TrustServiceError::QuotaExceeded),
            "expected QuotaExceeded, got: {err}"
        );
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

    #[tokio::test]
    async fn denounce_invalid_reason_carries_denouncement_reason_max_len_as_max() {
        // Verifies that InvalidReason carries DENOUNCEMENT_REASON_MAX_LEN as its
        // `max` field, not a hardcoded value or zero. This is consistent with how
        // DenouncementSlotsExhausted is tested (which also verifies its `max` field).
        // A refactor that accidentally passed a different constant would not be
        // caught by the existing InvalidReason tests that use `{ .. }`.
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let err = make_service().denounce(a, b, "").await.unwrap_err();
        assert!(
            matches!(
                err,
                TrustServiceError::InvalidReason { max }
                    if max == DENOUNCEMENT_REASON_MAX_LEN
            ),
            "expected InvalidReason {{ max: {DENOUNCEMENT_REASON_MAX_LEN} }}, got: {err}"
        );
    }

    #[tokio::test]
    async fn denounce_accepts_reason_at_exactly_max_len() {
        // The boundary is inclusive: exactly DENOUNCEMENT_REASON_MAX_LEN characters
        // must be accepted. This catches an off-by-one if the service-layer guard
        // ever uses `<` instead of `<=` in its length check.
        //
        // Uses StubTrustRepo (passes all guards) to verify the action is
        // actually enqueued rather than rejected.
        let captured = Arc::new(Mutex::new(None));
        let svc = DefaultTrustService::new(
            Arc::new(
                StubTrustRepo::default()
                    .daily(0)
                    .active(false)
                    .total(0)
                    .capture_payload(captured.clone()),
            ),
            Arc::new(StubReputationRepo::default()),
        );
        let accuser = Uuid::new_v4();
        let target = Uuid::new_v4();
        let reason = "x".repeat(DENOUNCEMENT_REASON_MAX_LEN);
        svc.denounce(accuser, target, &reason).await.expect(
            "denounce must accept reason at exactly DENOUNCEMENT_REASON_MAX_LEN characters",
        );
        let payload = captured.lock().unwrap().clone().unwrap();
        assert_eq!(
            payload["reason"].as_str().unwrap().chars().count(),
            DENOUNCEMENT_REASON_MAX_LEN,
            "payload reason must carry the full max-length string"
        );
    }

    #[tokio::test]
    async fn denounce_returns_slots_exhausted_when_at_denouncement_limit() {
        // The permanent denouncement budget (d=2) is full. The guard must fire
        // before enqueue_action is called — StubTrustRepo panics
        // on enqueue_action, so a missing guard would cause the test to panic.
        let svc = DefaultTrustService::new(
            Arc::new(
                StubTrustRepo::default()
                    .daily(0)
                    .active(false)
                    .total(i64::from(DENOUNCEMENT_SLOT_LIMIT)),
            ),
            Arc::new(StubReputationRepo::default()),
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

    #[tokio::test]
    async fn denounce_proceeds_when_one_denouncement_slot_remains() {
        // Lower boundary of the slot check: total = max - 1 (one slot still available).
        // The guard is `total >= max_denouncement_slots`, so this must NOT fire.
        // This is the complement of denounce_returns_slots_exhausted_when_at_denouncement_limit,
        // which tests the upper boundary (total == max → blocked). Together they pin the `>=`
        // comparison: if it were changed to `>`, the upper-boundary test would catch it;
        // if a +1 offset were added (e.g. `total + 1 >= max`), this lower-boundary test
        // would catch it.
        let captured = Arc::new(Mutex::new(None::<serde_json::Value>));
        let svc = DefaultTrustService::new(
            Arc::new(
                StubTrustRepo::default()
                    .daily(0)
                    .active(false)
                    .total(i64::from(DENOUNCEMENT_SLOT_LIMIT) - 1)
                    .capture_payload(captured.clone()),
            ),
            Arc::new(StubReputationRepo::default()),
        );
        let accuser = Uuid::new_v4();
        let target = Uuid::new_v4();
        svc.denounce(accuser, target, "valid reason")
            .await
            .expect("denounce must succeed when one denouncement slot remains");
        assert!(
            captured.lock().unwrap().is_some(),
            "enqueue_action must be called when denouncement slots remain"
        );
    }

    #[tokio::test]
    async fn denounce_returns_quota_exceeded_when_daily_limit_reached() {
        // The daily action quota is exhausted. The guard must fire before
        // count_total_denouncements_by is reached — StubTrustRepo
        // panics on count_total_denouncements_by, so a missing guard causes the
        // test to panic rather than silently pass.
        let svc = DefaultTrustService::new(
            Arc::new(
                StubTrustRepo::default()
                    .daily(DAILY_ACTION_QUOTA)
                    .active(false),
            ),
            Arc::new(StubReputationRepo::default()),
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

    #[tokio::test]
    async fn denounce_propagates_repo_error_from_has_active_denouncement() {
        // `has_active_denouncement` is the first repo call in `denounce` (after the
        // pure self-action and reason checks). If the database fails there, the
        // service must surface the error rather than silently succeeding or panicking.
        // `StubTrustRepo` panics on every other method, so a
        // missing `?` would cause the test to fail loudly.
        let svc = DefaultTrustService::new(
            Arc::new(StubTrustRepo::default().active_error()),
            Arc::new(StubReputationRepo::default()),
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
    async fn denounce_propagates_repo_error_from_count_daily_actions() {
        // `count_daily_actions` is called inside `check_daily_quota`, which runs
        // after `has_active_denouncement` passes. If the database fails there, the
        // service must surface the error rather than silently succeeding or panicking.
        // `StubTrustRepo` panics on `count_total_denouncements_by`, so a
        // missing `?` would cause the test to fail loudly.
        let svc = DefaultTrustService::new(
            Arc::new(StubTrustRepo::default().active(false).daily_error()),
            Arc::new(StubReputationRepo::default()),
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

    #[tokio::test]
    async fn endorse_propagates_repo_error_from_count_daily_actions() {
        // `count_daily_actions` is the first repo call in `endorse` (after the
        // pure self-action and weight checks). If the database fails there, the
        // service must surface the error rather than silently succeeding or panicking.
        // `StubTrustRepo` panics on every other method, so a
        // missing `?` would cause the test to fail loudly.
        let svc = DefaultTrustService::new(
            Arc::new(StubTrustRepo::default().daily_error()),
            Arc::new(StubReputationRepo::default()),
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
    async fn endorse_propagates_repo_error_from_has_active_denouncement() {
        // `has_active_denouncement` is called after `count_daily_actions` passes.
        // If the database fails there, the service must surface the error rather
        // than silently succeeding or panicking.
        // `StubTrustRepo` panics on every other method,
        // so a missing `?` would cause the test to fail loudly.
        let svc = DefaultTrustService::new(
            Arc::new(StubTrustRepo::default().daily(0).active_error()),
            Arc::new(StubReputationRepo::default()),
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

    #[tokio::test]
    async fn endorse_queues_with_in_slot_true_for_verifier() {
        // Verifier accounts are exempt from the k=3 endorsement slot limit.
        // Even with active_endorsements == ENDORSEMENT_SLOT_LIMIT, the queued
        // payload must carry `in_slot = true`.
        let captured = Arc::new(Mutex::new(None));
        let svc = DefaultTrustService::new(
            Arc::new(
                StubTrustRepo::default()
                    .daily(0)
                    .active(false)
                    .capture_payload(captured.clone()),
            ),
            Arc::new(
                StubReputationRepo::default()
                    .verifier(true)
                    .active_count(i64::from(ENDORSEMENT_SLOT_LIMIT)),
            ), // slots "full"
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
            Arc::new(
                StubTrustRepo::default()
                    .daily(0)
                    .active(false)
                    .capture_payload(captured.clone()),
            ),
            Arc::new(
                StubReputationRepo::default()
                    .verifier(false)
                    .active_count(i64::from(ENDORSEMENT_SLOT_LIMIT)),
            ), // at slot limit
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
            Arc::new(
                StubTrustRepo::default()
                    .daily(0)
                    .active(false)
                    .capture_payload(captured.clone()),
            ),
            Arc::new(
                StubReputationRepo::default()
                    .verifier(false)
                    .active_count(i64::from(ENDORSEMENT_SLOT_LIMIT) - 1),
            ), // one slot free
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
            Arc::new(
                StubTrustRepo::default()
                    .daily(0)
                    .active(false)
                    .capture_payload(captured.clone()),
            ),
            Arc::new(
                StubReputationRepo::default()
                    .verifier(false)
                    .active_count(0),
            ),
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
            Arc::new(
                StubTrustRepo::default()
                    .daily(0)
                    .active(false)
                    .capture_payload(captured.clone()),
            ),
            Arc::new(
                StubReputationRepo::default()
                    .verifier(false)
                    .active_count(0),
            ),
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
            Arc::new(
                StubTrustRepo::default()
                    .daily(0)
                    .active(false)
                    .total(0)
                    .capture_payload(captured.clone()),
            ),
            Arc::new(StubReputationRepo::default()),
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

    // ─── count_active_trust_endorsements_by error propagation ────────────────

    #[tokio::test]
    async fn endorse_propagates_endorsement_repo_error_when_active_count_check_fails() {
        // When the caller is not a verifier (`has_endorsement` → Ok(false)),
        // `endorse` calls `count_active_trust_endorsements_by` to determine the
        // `in_slot` flag. If that call fails, the error must be propagated as
        // `TrustServiceError::EndorsementRepo` rather than silently treating the
        // count as 0 and proceeding to enqueue. `StubTrustRepo` panics on
        // `enqueue_action`, so a missing `?` here would surface immediately.
        let captured = Arc::new(Mutex::new(None));
        let svc = DefaultTrustService::new(
            Arc::new(
                StubTrustRepo::default()
                    .daily(0)
                    .active(false)
                    .capture_payload(captured.clone()),
            ),
            Arc::new(
                StubReputationRepo::default()
                    .verifier(false)
                    .active_count_error(),
            ),
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

    #[tokio::test]
    async fn denounce_propagates_repo_error_from_count_total_denouncements_by() {
        // `count_total_denouncements_by` is called after `has_active_denouncement`
        // and `count_daily_actions` pass. If the database fails there, the service
        // must surface the error rather than silently succeeding or panicking.
        // `StubTrustRepo` panics on `enqueue_action`, so a
        // missing `?` on the repo call would cause the test to panic loudly.
        let svc = DefaultTrustService::new(
            Arc::new(
                StubTrustRepo::default()
                    .daily(0)
                    .active(false)
                    .total_error(),
            ),
            Arc::new(StubReputationRepo::default()),
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
        // Uses `StubTrustRepo` as the TrustRepo stub so all upstream guards
        // (quota, denouncement conflict) pass, ensuring the path reaches
        // `has_endorsement` before the error is returned.
        let captured = Arc::new(Mutex::new(None));
        let svc = DefaultTrustService::new(
            Arc::new(
                StubTrustRepo::default()
                    .daily(0)
                    .active(false)
                    .capture_payload(captured.clone()),
            ),
            Arc::new(StubReputationRepo::default().verifier_error()),
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

    #[tokio::test]
    async fn endorse_propagates_repo_error_from_enqueue_action() {
        // `enqueue_action` is the last repo call in `endorse` — all guards pass
        // (self-action, weight, daily quota, denouncement conflict, and the
        // verifier/slot checks). If the database fails at the enqueue step, the
        // service must surface the error rather than silently succeeding.
        // `StubTrustRepo` panics on all other methods, so a stray
        // call would cause the test to fail loudly.
        let svc = DefaultTrustService::new(
            Arc::new(
                StubTrustRepo::default()
                    .daily(0)
                    .active(false)
                    .enqueue_error(),
            ),
            Arc::new(
                StubReputationRepo::default()
                    .verifier(false)
                    .active_count(0),
            ),
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
        // than silently succeeding. `StubTrustRepo` panics on all other
        // methods, so a stray call would cause the test to fail loudly.
        let svc = DefaultTrustService::new(
            Arc::new(StubTrustRepo::default().daily(0).enqueue_error()),
            Arc::new(StubReputationRepo::default()),
        );
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let err = svc.revoke_endorsement(a, b).await.unwrap_err();
        assert!(
            matches!(err, TrustServiceError::Repo(TrustRepoError::Database(_))),
            "expected Repo(Database(...)), got: {err}"
        );
    }

    // ─── denounce enqueue_action error propagation ────────────────────────────

    #[tokio::test]
    async fn denounce_propagates_repo_error_from_enqueue_action() {
        // `enqueue_action` is the last repo call in `denounce` — all guards pass
        // (self-action, reason validation, `has_active_denouncement`, daily quota,
        // and slot limit checks). If the database fails at the enqueue step, the
        // service must surface the error rather than silently succeeding.
        // `StubTrustRepo` panics on all other methods, so a stray
        // call would cause the test to fail loudly.
        let svc = DefaultTrustService::new(
            Arc::new(
                StubTrustRepo::default()
                    .daily(0)
                    .active(false)
                    .total(0)
                    .enqueue_error(),
            ),
            Arc::new(StubReputationRepo::default()),
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

    #[tokio::test]
    async fn endorse_enqueues_action_type_endorse() {
        // Verifies that endorse() passes ActionType::Endorse to enqueue_action.
        // Without this pin, a copy-paste that uses ActionType::Revoke would compile,
        // pass all other tests, and cause the worker to process endorsements as revokes.
        let captured_type = Arc::new(Mutex::new(None));
        let svc = DefaultTrustService::new(
            Arc::new(
                StubTrustRepo::default()
                    .daily(0)
                    .active(false)
                    .total(0)
                    .capture_type(captured_type.clone()),
            ),
            Arc::new(
                StubReputationRepo::default()
                    .verifier(false)
                    .active_count(0),
            ),
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
        // Only `.daily(0)` and `.capture_type(...)` are set: `revoke_endorsement` does not
        // call `has_active_denouncement` or `count_total_denouncements_by`, so those methods
        // are left unset (unimplemented!()) to catch any unexpected calls.
        let captured_type = Arc::new(Mutex::new(None));
        let svc = DefaultTrustService::new(
            Arc::new(
                StubTrustRepo::default()
                    .daily(0)
                    .capture_type(captured_type.clone()),
            ),
            Arc::new(StubReputationRepo::default()),
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
            Arc::new(
                StubTrustRepo::default()
                    .daily(0)
                    .active(false)
                    .total(0)
                    .capture_type(captured_type.clone()),
            ),
            Arc::new(StubReputationRepo::default()),
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

    // ─── revoke_endorsement actor_id correctness ─────────────────────────────

    #[tokio::test]
    async fn revoke_endorsement_passes_endorser_as_actor_to_enqueue_action() {
        // Verifies that revoke_endorsement() passes endorser_id (not subject_id)
        // as the actor argument to enqueue_action. The payload-content tests only
        // check the payload fields; a bug that swaps endorser_id/subject_id in
        // the actor position would silently pass all other tests but attribute the
        // revocation to the subject's account in the action log.
        let captured_actor = Arc::new(Mutex::new(None::<Uuid>));
        let svc = DefaultTrustService::new(
            Arc::new(
                StubTrustRepo::default()
                    .daily(0)
                    .capture_actor(captured_actor.clone()),
            ),
            Arc::new(StubReputationRepo::default()),
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
            Arc::new(
                StubTrustRepo::default()
                    .active(false)
                    .daily(0)
                    .total(0)
                    .capture_actor(captured_actor.clone()),
            ),
            Arc::new(StubReputationRepo::default()),
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

    // ─── endorse actor_id correctness ─────────────────────────────────────────

    #[tokio::test]
    async fn endorse_passes_endorser_as_actor_to_enqueue_action() {
        // Verifies that endorse() passes endorser_id (not subject_id) as the actor
        // argument to enqueue_action. The payload-content tests only check the
        // payload fields; a bug that swaps endorser_id/subject_id in the actor
        // position would silently pass all other tests but attribute the
        // endorsement to the subject's account in the action log.
        let captured_actor = Arc::new(Mutex::new(None::<Uuid>));
        let svc = DefaultTrustService::new(
            Arc::new(
                StubTrustRepo::default()
                    .daily(0)
                    .active(false)
                    .capture_actor(captured_actor.clone()),
            ),
            Arc::new(
                StubReputationRepo::default()
                    .verifier(false)
                    .active_count(0),
            ),
        );
        let endorser = Uuid::new_v4();
        let subject = Uuid::new_v4();
        svc.endorse(endorser, subject, 0.5, None).await.unwrap();
        assert_eq!(
            *captured_actor.lock().unwrap(),
            Some(endorser),
            "endorse must pass endorser_id as actor, not subject_id"
        );
    }

    // ─── daily quota actor correctness ───────────────────────────────────────

    /// Stub [`TrustRepo`] that captures the `actor_id` argument passed to
    /// `count_daily_actions`. Used to verify that `endorse` applies the daily
    /// quota check against `endorser_id` (not `subject_id`): a bug that passed
    /// `subject_id` would allow an endorser to bypass their quota if the
    /// subject still has capacity.
    ///

    #[tokio::test]
    async fn endorse_passes_endorser_to_count_daily_actions() {
        // Verifies that endorse() applies the daily quota check against
        // endorser_id, not subject_id. A bug that passed subject_id would
        // allow an endorser who has hit their quota to endorse indefinitely
        // as long as their targets still have capacity — silently bypassing
        // the rate limit. All other endorse tests use repos that ignore the
        // actor_id argument to count_daily_actions, so this gap would not
        // be caught without an explicit capturing test.
        //
        // The enqueue_action stub returns an error to terminate the call
        // early; we assert on the captured actor before checking the result.
        let captured_quota_actor = Arc::new(Mutex::new(None::<Uuid>));
        let svc = DefaultTrustService::new(
            Arc::new(
                StubTrustRepo::default()
                    .daily(0)
                    .active(false)
                    .capture_daily_actor(captured_quota_actor.clone())
                    .enqueue_error(),
            ),
            Arc::new(
                StubReputationRepo::default()
                    .verifier(false)
                    .active_count(0),
            ),
        );
        let endorser = Uuid::new_v4();
        let subject = Uuid::new_v4();
        // Result is an error from enqueue_action — expected; we only care about
        // the quota actor.
        let _ = svc.endorse(endorser, subject, 0.5, None).await;
        assert_eq!(
            *captured_quota_actor.lock().unwrap(),
            Some(endorser),
            "endorse must check the daily quota for endorser_id, not subject_id"
        );
    }

    // ─── revoke daily quota actor correctness ────────────────────────────────

    /// Stub [`TrustRepo`] that captures the `actor_id` argument passed to
    /// `count_daily_actions`. Used to verify that `revoke_endorsement` applies the
    /// daily quota check against `endorser_id` (not `subject_id`): a bug that passed
    /// `subject_id` would allow an endorser to bypass their quota if the
    /// subject still has capacity.
    ///

    #[tokio::test]
    async fn revoke_endorsement_passes_endorser_to_count_daily_actions() {
        // Verifies that revoke_endorsement() applies the daily quota check against
        // endorser_id, not subject_id. A bug that passed subject_id would allow an
        // endorser who has hit their quota to keep revoking as long as their targets
        // still have capacity — silently bypassing the rate limit. All other
        // revoke_endorsement tests use repos that ignore the actor_id argument to
        // count_daily_actions, so this gap would not be caught without an explicit
        // capturing test.
        //
        // The enqueue_action stub returns an error to terminate the call early;
        // we assert on the captured actor before checking the result.
        let captured_quota_actor = Arc::new(Mutex::new(None::<Uuid>));
        let svc = DefaultTrustService::new(
            Arc::new(
                StubTrustRepo::default()
                    .daily(0)
                    .capture_daily_actor(captured_quota_actor.clone())
                    .enqueue_error(),
            ),
            Arc::new(StubReputationRepo::default()),
        );
        let endorser = Uuid::new_v4();
        let subject = Uuid::new_v4();
        // Result is an error from enqueue_action — expected; we only care about
        // the quota actor.
        let _ = svc.revoke_endorsement(endorser, subject).await;
        assert_eq!(
            *captured_quota_actor.lock().unwrap(),
            Some(endorser),
            "revoke_endorsement must check the daily quota for endorser_id, not subject_id"
        );
    }

    // ─── denounce daily quota actor correctness ───────────────────────────────

    /// Stub [`TrustRepo`] that passes all pre-enqueue guards in
    /// [`DefaultTrustService::denounce`] (active denouncement and denouncement
    /// slot checks) and captures the `actor_id` argument passed to
    /// `count_daily_actions`. Used to verify that `denounce` applies the daily
    /// quota check against `accuser_id` (not `target_id`): a bug that passed
    /// `target_id` would allow an accuser who has hit their quota to keep filing
    /// denouncements as long as their targets still have quota capacity —
    /// silently bypassing the rate limit.
    ///

    #[tokio::test]
    async fn denounce_passes_accuser_to_count_daily_actions() {
        // Verifies that denounce() applies the daily quota check against
        // accuser_id, not target_id. A bug that passed target_id would allow an
        // accuser who has hit their quota to keep filing denouncements as long as
        // their targets still have capacity — silently bypassing the rate limit.
        // All other denounce tests use repos that ignore the actor_id argument to
        // count_daily_actions, so this gap would not be caught without an explicit
        // capturing test.
        //
        // The enqueue_action stub returns an error to terminate the call early;
        // we assert on the captured actor before checking the result.
        let captured_quota_actor = Arc::new(Mutex::new(None::<Uuid>));
        let svc = DefaultTrustService::new(
            Arc::new(
                StubTrustRepo::default()
                    .daily(0)
                    .active(false)
                    .total(0)
                    .capture_daily_actor(captured_quota_actor.clone())
                    .enqueue_error(),
            ),
            Arc::new(StubReputationRepo::default()),
        );
        let accuser = Uuid::new_v4();
        let target = Uuid::new_v4();
        // Result is an error from enqueue_action — expected; we only care about
        // the quota actor.
        let _ = svc.denounce(accuser, target, "valid reason").await;
        assert_eq!(
            *captured_quota_actor.lock().unwrap(),
            Some(accuser),
            "denounce must check the daily quota for accuser_id, not target_id"
        );
    }

    // ─── denounce has_active_denouncement ID-order correctness ───────────────

    #[tokio::test]
    async fn denounce_passes_accuser_id_and_target_id_to_has_active_denouncement() {
        // Verifies that denounce() passes (accuser_id, target_id) — not
        // (target_id, accuser_id) — to has_active_denouncement. A swap would
        // query the wrong direction of the denouncement relationship: it would
        // check whether the target has already denounced the accuser, not
        // whether the accuser has already denounced the target. That would
        // allow the accuser to file duplicate denouncements undetected.
        // All other denounce tests use repos that ignore the ID arguments to
        // has_active_denouncement, so this gap would not be caught without an
        // explicit capturing test.
        //
        // The stub returns an error from has_active_denouncement to terminate
        // the call early; we assert on the captured IDs before checking the
        // result.
        let captured = Arc::new(Mutex::new(None::<(Uuid, Uuid)>));
        let svc = DefaultTrustService::new(
            Arc::new(
                StubTrustRepo::default()
                    .active_error()
                    .capture_active(captured.clone()),
            ),
            Arc::new(StubReputationRepo::default()),
        );
        let accuser = Uuid::new_v4();
        let target = Uuid::new_v4();
        // Result is an error from has_active_denouncement — expected; we only
        // care about the captured IDs.
        let _ = svc.denounce(accuser, target, "valid reason").await;
        let ids = captured.lock().unwrap();
        let (first, second) = ids.expect("has_active_denouncement must have been called");
        assert_eq!(
            first, accuser,
            "denounce must pass accuser_id as the first arg to has_active_denouncement"
        );
        assert_eq!(
            second, target,
            "denounce must pass target_id as the second arg to has_active_denouncement"
        );
    }

    // ─── denounce count_total_denouncements_by actor-id correctness ──────────

    /// Stub [`TrustRepo`] that passes `has_active_denouncement` and
    /// `count_daily_actions`, then captures the actor_id passed to
    /// `count_total_denouncements_by` and returns an error to terminate early.
    ///

    // ─── endorse has_active_denouncement actor-id correctness ────────────────

    /// Stub [`TrustRepo`] that passes `count_daily_actions`, then captures both
    /// IDs passed to `has_active_denouncement` and returns an error to terminate
    /// early.
    ///

    #[tokio::test]
    async fn endorse_passes_endorser_id_and_subject_id_to_has_active_denouncement() {
        // Verifies that endorse() passes (endorser_id, subject_id) — not
        // (subject_id, endorser_id) — to has_active_denouncement. A swap would
        // query the wrong direction: whether the subject has already denounced
        // the endorser instead of whether the endorser has denounced the subject.
        // That would allow an endorsement to proceed when it should be blocked by
        // DenouncementConflict. All other endorse tests use repos that ignore the
        // ID arguments to has_active_denouncement, so this gap would not be caught
        // without an explicit capturing test.
        //
        // The stub returns an error from has_active_denouncement to terminate the
        // call early; we assert on the captured IDs before checking the result.
        let captured = Arc::new(Mutex::new(None::<(Uuid, Uuid)>));
        let svc = DefaultTrustService::new(
            Arc::new(
                StubTrustRepo::default()
                    .daily(0)
                    .active_error()
                    .capture_active(captured.clone()),
            ),
            Arc::new(StubReputationRepo::default()),
        );
        let endorser = Uuid::new_v4();
        let subject = Uuid::new_v4();
        // Result is an error from has_active_denouncement — expected; we only
        // care about the captured IDs.
        let _ = svc.endorse(endorser, subject, 0.5, None).await;
        let ids = captured.lock().unwrap();
        let (first, second) = ids.expect("has_active_denouncement must have been called");
        assert_eq!(
            first, endorser,
            "endorse must pass endorser_id as the first arg to has_active_denouncement"
        );
        assert_eq!(
            second, subject,
            "endorse must pass subject_id as the second arg to has_active_denouncement"
        );
    }

    // ─── endorse verifier-check actor-id correctness ─────────────────────────

    #[tokio::test]
    async fn endorse_passes_endorser_id_to_has_endorsement() {
        // Verifies that endorse() checks the *endorser's* verifier status, not
        // the subject's. A swap would grant slot-limit exemption whenever the
        // subject is a verifier rather than the endorser. All other verifier-check
        // tests use repos that ignore which user_id is passed to has_endorsement
        // (`StubReputationRepo` without a capture), so this gap would not
        // be caught without an explicit capturing test.
        //
        // Uses StubTrustRepo for TrustRepo (passes count_daily_actions and
        // has_active_denouncement guards). StubReputationRepo with
        // capture_endorsement_id + verifier_error terminates early; we only care
        // about the captured ID.
        let captured_id = Arc::new(Mutex::new(None::<Uuid>));
        let svc = DefaultTrustService::new(
            Arc::new(
                StubTrustRepo::default()
                    .daily(0)
                    .active(false)
                    .capture_payload(Arc::new(Mutex::new(None))),
            ),
            Arc::new(
                StubReputationRepo::default()
                    .capture_endorsement_id(captured_id.clone())
                    .verifier_error(),
            ),
        );
        let endorser = Uuid::new_v4();
        let subject = Uuid::new_v4();
        // Result is an error from has_endorsement — expected; we only care about
        // which ID was passed.
        let _ = svc.endorse(endorser, subject, 0.5, None).await;
        assert_eq!(
            *captured_id.lock().unwrap(),
            Some(endorser),
            "endorse must check the endorser's verifier status, not the subject's"
        );
    }

    // ─── endorse verifier-check topic correctness ────────────────────────────

    #[tokio::test]
    async fn endorse_passes_authorized_verifier_topic_to_has_endorsement() {
        // Verifies that endorse() queries the "authorized_verifier" topic when
        // checking slot-limit exemption. A wrong topic (e.g. "trust" or "verifier")
        // would silently grant or deny the exemption to the wrong users. All other
        // endorse tests use stubs that discard the topic argument (`_: &str`), so
        // this invariant is not covered elsewhere.
        //
        // Uses StubTrustRepo for TrustRepo (passes count_daily_actions and
        // has_active_denouncement guards). StubReputationRepo with
        // capture_endorsement_topic + verifier_error terminates early; we only
        // care about the captured topic.
        let captured_topic = Arc::new(Mutex::new(None::<String>));
        let svc = DefaultTrustService::new(
            Arc::new(
                StubTrustRepo::default()
                    .daily(0)
                    .active(false)
                    .capture_payload(Arc::new(Mutex::new(None))),
            ),
            Arc::new(
                StubReputationRepo::default()
                    .capture_endorsement_topic(captured_topic.clone())
                    .verifier_error(),
            ),
        );
        let endorser = Uuid::new_v4();
        let subject = Uuid::new_v4();
        // Result is an error from has_endorsement — expected; we only care about
        // which topic was passed.
        let _ = svc.endorse(endorser, subject, 0.5, None).await;
        assert_eq!(
            captured_topic.lock().unwrap().as_deref(),
            Some("authorized_verifier"),
            "endorse must query the 'authorized_verifier' topic, not some other string"
        );
    }

    // ─── endorse active-slot-count actor-id correctness ──────────────────────

    #[tokio::test]
    async fn endorse_passes_endorser_id_to_count_active_trust_endorsements_by() {
        // Verifies that endorse() checks the *endorser's* active slot count, not
        // the subject's. A swap would check whether the subject still has available
        // endorsement slots — granting in_slot = true to an endorser who has
        // exhausted their own slots, as long as the subject has capacity. All other
        // endorse tests use repos that ignore the actor_id argument to
        // count_active_trust_endorsements_by, so this gap would not be caught
        // without an explicit capturing test.
        //
        // StubTrustRepo provides the TrustRepo (passes count_daily_actions
        // and has_active_denouncement guards). StubReputationRepo with
        // verifier(false) + capture_active_id + active_count_error returns Ok(false)
        // from has_endorsement (non-verifier path) and captures the actor_id passed
        // to count_active_trust_endorsements_by, then returns an error to terminate
        // early.
        let captured_id = Arc::new(Mutex::new(None::<Uuid>));
        let svc = DefaultTrustService::new(
            Arc::new(
                StubTrustRepo::default()
                    .daily(0)
                    .active(false)
                    .capture_payload(Arc::new(Mutex::new(None))),
            ),
            Arc::new(
                StubReputationRepo::default()
                    .verifier(false)
                    .capture_active_id(captured_id.clone())
                    .active_count_error(),
            ),
        );
        let endorser = Uuid::new_v4();
        let subject = Uuid::new_v4();
        // Result is an error from count_active_trust_endorsements_by — expected;
        // we only care about which ID was passed.
        let _ = svc.endorse(endorser, subject, 0.5, None).await;
        assert_eq!(
            *captured_id.lock().unwrap(),
            Some(endorser),
            "endorse must check the endorser's active slot count, not the subject's"
        );
    }

    #[tokio::test]
    async fn denounce_passes_accuser_to_count_total_denouncements_by() {
        // Verifies that denounce() checks the permanent denouncement slot budget
        // for accuser_id, not target_id. A bug that passed target_id would check
        // the target's budget — allowing the accuser to bypass the permanent d=2
        // slot limit by targeting users who haven't used their own slots. All
        // other denounce tests use repos that ignore the actor_id argument to
        // count_total_denouncements_by, so this gap would not be caught without
        // an explicit capturing test.
        //
        // The stub returns an error from count_total_denouncements_by to terminate
        // the call early; we assert on the captured actor before checking the result.
        let captured_slot_actor = Arc::new(Mutex::new(None::<Uuid>));
        let svc = DefaultTrustService::new(
            Arc::new(
                StubTrustRepo::default()
                    .active(false)
                    .daily(0)
                    .total_error()
                    .capture_total_actor(captured_slot_actor.clone()),
            ),
            Arc::new(StubReputationRepo::default()),
        );
        let accuser = Uuid::new_v4();
        let target = Uuid::new_v4();
        // Result is an error from count_total_denouncements_by — expected; we
        // only care about the captured actor.
        let _ = svc.denounce(accuser, target, "valid reason").await;
        assert_eq!(
            *captured_slot_actor.lock().unwrap(),
            Some(accuser),
            "denounce must check the slot budget for accuser_id, not target_id"
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

    /// Check whether `actor_id` has reached the daily action quota.
    ///
    /// Returns `Ok(())` when the actor is below the limit, or
    /// `Err(TrustServiceError::QuotaExceeded)` when the limit is reached.
    /// Propagates any repo error as `TrustServiceError::Repo`.
    async fn check_daily_quota(&self, actor_id: Uuid) -> Result<(), TrustServiceError> {
        let daily_count = self.trust_repo.count_daily_actions(actor_id).await?;
        if daily_count >= self.daily_quota {
            return Err(TrustServiceError::QuotaExceeded);
        }
        Ok(())
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

        self.check_daily_quota(endorser_id).await?;

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

        self.check_daily_quota(endorser_id).await?;

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

        self.check_daily_quota(accuser_id).await?;

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
