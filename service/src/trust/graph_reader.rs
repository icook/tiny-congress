//! Adapter that bridges [`TrustRepo`] to the engine-api [`TrustGraphReader`] trait.

use std::sync::Arc;

use tc_engine_api::trust::{TrustGraphReader, TrustScoreSnapshot};
use uuid::Uuid;

use super::repo::TrustRepo;

/// Thin adapter implementing [`TrustGraphReader`] by delegating to a [`TrustRepo`].
pub struct TrustRepoGraphReader {
    trust_repo: Arc<dyn TrustRepo>,
}

impl TrustRepoGraphReader {
    #[must_use]
    pub fn new(trust_repo: Arc<dyn TrustRepo>) -> Self {
        Self { trust_repo }
    }
}

#[async_trait::async_trait]
impl TrustGraphReader for TrustRepoGraphReader {
    async fn get_score(
        &self,
        subject: Uuid,
        anchor: Option<Uuid>,
    ) -> Result<Option<TrustScoreSnapshot>, anyhow::Error> {
        let snapshot = self
            .trust_repo
            .get_score(subject, anchor)
            .await
            .map_err(anyhow::Error::new)?;

        let mapped = snapshot.and_then(|s| {
            let Some(trust_distance_raw) = s.trust_distance else {
                tracing::warn!(
                    subject = %subject,
                    "NULL trust_distance in score snapshot — possible data corruption; treating as no score"
                );
                return None;
            };
            if trust_distance_raw < 0.0 {
                tracing::warn!(
                    subject = %subject,
                    trust_distance = trust_distance_raw,
                    "negative trust_distance — possible data corruption; treating as no score"
                );
                return None;
            }
            if !trust_distance_raw.is_finite() {
                tracing::warn!(
                    subject = %subject,
                    trust_distance = trust_distance_raw,
                    "non-finite trust_distance — possible data corruption; treating as no score"
                );
                return None;
            }
            let raw = s.path_diversity.unwrap_or(0);
            u32::try_from(raw).map_or_else(
                |_| {
                    tracing::warn!(
                        subject = %subject,
                        path_diversity = raw,
                        "negative path_diversity — possible data corruption; treating as no score"
                    );
                    None
                },
                |path_diversity| {
                    let ec_raw = s.eigenvector_centrality.unwrap_or(0.0);
                    if ec_raw < 0.0 {
                        tracing::warn!(
                            subject = %subject,
                            eigenvector_centrality = ec_raw,
                            "negative eigenvector_centrality — possible data corruption; treating as no score"
                        );
                        return None;
                    }
                    if !ec_raw.is_finite() {
                        tracing::warn!(
                            subject = %subject,
                            eigenvector_centrality = ec_raw,
                            "non-finite eigenvector_centrality — possible data corruption; treating as no score"
                        );
                        return None;
                    }
                    Some(TrustScoreSnapshot {
                        trust_distance: f64::from(trust_distance_raw),
                        path_diversity,
                        eigenvector_centrality: f64::from(ec_raw),
                    })
                },
            )
        });
        Ok(mapped)
    }

    async fn has_endorsement(
        &self,
        subject: Uuid,
        topic: &str,
        verifier_ids: &[Uuid],
    ) -> Result<bool, anyhow::Error> {
        self.trust_repo
            .has_identity_endorsement(subject, verifier_ids, topic)
            .await
            .map_err(anyhow::Error::new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trust::repo::{
        ActionRecord, DenouncementRecord, DenouncementWithUsername, InfluenceRecord, InviteRecord,
        ScoreSnapshot, TrustRepo, TrustRepoError,
    };
    use crate::trust::service::ActionType;
    use crate::trust::weight::{DeliveryMethod, RelationshipDepth};
    use async_trait::async_trait;

    /// Single configurable stub for [`TrustRepo`] covering all graph-reader test scenarios.
    ///
    /// Enable the field(s) relevant to your test; all other methods panic with
    /// `unimplemented!()`. When `TrustRepo` gains a new method that graph-reader
    /// tests need to exercise, add one field here rather than updating every
    /// separate stub.
    #[derive(Default)]
    struct StubTrustRepo {
        score: Option<ScoreSnapshot>,
        score_fails: bool,
        endorsement: Option<bool>,
        endorsement_fails: bool,
    }

    impl StubTrustRepo {
        fn with_score(snapshot: Option<ScoreSnapshot>) -> Self {
            Self {
                score: snapshot,
                ..Default::default()
            }
        }
        fn with_score_error() -> Self {
            Self {
                score_fails: true,
                ..Default::default()
            }
        }
        fn with_endorsement(value: bool) -> Self {
            Self {
                endorsement: Some(value),
                ..Default::default()
            }
        }
        fn with_endorsement_error() -> Self {
            Self {
                endorsement_fails: true,
                ..Default::default()
            }
        }
    }

    #[async_trait]
    impl TrustRepo for StubTrustRepo {
        async fn get_score(
            &self,
            _: Uuid,
            _: Option<Uuid>,
        ) -> Result<Option<ScoreSnapshot>, TrustRepoError> {
            if self.score_fails {
                return Err(TrustRepoError::Database(sqlx::Error::RowNotFound));
            }
            Ok(self.score.clone())
        }
        async fn has_identity_endorsement(
            &self,
            _: Uuid,
            _: &[Uuid],
            _: &str,
        ) -> Result<bool, TrustRepoError> {
            if self.endorsement_fails {
                return Err(TrustRepoError::Database(sqlx::Error::RowNotFound));
            }
            Ok(self.endorsement.unwrap_or(false))
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
        async fn get_all_scores(&self, _: Uuid) -> Result<Vec<ScoreSnapshot>, TrustRepoError> {
            unimplemented!()
        }
    }

    fn make_reader(snapshot: Option<ScoreSnapshot>) -> TrustRepoGraphReader {
        TrustRepoGraphReader::new(Arc::new(StubTrustRepo::with_score(snapshot)))
    }

    fn make_endorsement_reader(result: Result<bool, TrustRepoError>) -> TrustRepoGraphReader {
        let stub = match result {
            Ok(v) => StubTrustRepo::with_endorsement(v),
            Err(_) => StubTrustRepo::with_endorsement_error(),
        };
        TrustRepoGraphReader::new(Arc::new(stub))
    }

    #[tokio::test]
    async fn get_score_propagates_repo_error() {
        let reader = TrustRepoGraphReader::new(Arc::new(StubTrustRepo::with_score_error()));
        let result = reader.get_score(Uuid::new_v4(), None).await;
        assert!(
            result.is_err(),
            "expected get_score to propagate the repo error"
        );
    }

    fn base_snapshot() -> ScoreSnapshot {
        ScoreSnapshot {
            user_id: Uuid::new_v4(),
            context_user_id: None,
            trust_distance: Some(1.5),
            path_diversity: Some(2),
            eigenvector_centrality: Some(0.3),
            computed_at: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn get_score_returns_none_when_repo_returns_none() {
        let reader = make_reader(None);
        let result = reader.get_score(Uuid::new_v4(), None).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn get_score_maps_valid_snapshot_to_trust_score() {
        let snapshot = base_snapshot();
        let reader = make_reader(Some(snapshot));
        let result = reader.get_score(Uuid::new_v4(), None).await.unwrap();
        let score = result.expect("expected Some score");
        assert!((score.trust_distance - 1.5_f64).abs() < f64::EPSILON);
        assert_eq!(score.path_diversity, 2);
        assert!((score.eigenvector_centrality - 0.3_f64).abs() < 1e-6);
    }

    #[tokio::test]
    async fn get_score_returns_none_when_trust_distance_is_null() {
        // NULL trust_distance indicates data corruption — the adapter must treat the
        // snapshot as absent rather than silently returning distance=0.0 (which is
        // the anchor's own distance and semantically incorrect for any other user).
        let mut snapshot = base_snapshot();
        snapshot.trust_distance = None;
        let reader = make_reader(Some(snapshot));
        let result = reader.get_score(Uuid::new_v4(), None).await.unwrap();
        assert!(
            result.is_none(),
            "NULL trust_distance must map to no score, not distance=0.0"
        );
    }

    #[tokio::test]
    async fn get_score_returns_none_when_trust_distance_is_negative() {
        // Negative trust_distance cannot arise from a correct hop-count computation
        // (distances are always >= 0). Treating it as "no score" rather than silently
        // propagating a nonsensical negative distance follows the same fail-closed
        // approach applied to negative path_diversity.
        let mut snapshot = base_snapshot();
        snapshot.trust_distance = Some(-0.5);
        let reader = make_reader(Some(snapshot));
        let result = reader.get_score(Uuid::new_v4(), None).await.unwrap();
        assert!(
            result.is_none(),
            "negative trust_distance must map to no score (data corruption), not a negative distance"
        );
    }

    #[tokio::test]
    async fn get_score_returns_none_when_trust_distance_is_nan() {
        // NaN trust_distance must be rejected — it is not < 0.0, so without an
        // explicit is_finite() check it would silently propagate to the caller as
        // f64::NAN, which is nonsensical as a trust distance.
        let mut snapshot = base_snapshot();
        snapshot.trust_distance = Some(f32::NAN);
        let reader = make_reader(Some(snapshot));
        let result = reader.get_score(Uuid::new_v4(), None).await.unwrap();
        assert!(
            result.is_none(),
            "NaN trust_distance must map to no score (data corruption), not NaN"
        );
    }

    #[tokio::test]
    async fn get_score_returns_none_when_trust_distance_is_infinite() {
        // INFINITY trust_distance similarly cannot arise from correct computation
        // (distances are always finite) and must be treated as data corruption.
        let mut snapshot = base_snapshot();
        snapshot.trust_distance = Some(f32::INFINITY);
        let reader = make_reader(Some(snapshot));
        let result = reader.get_score(Uuid::new_v4(), None).await.unwrap();
        assert!(
            result.is_none(),
            "INFINITY trust_distance must map to no score (data corruption), not INFINITY"
        );
    }

    #[tokio::test]
    async fn get_score_returns_none_when_path_diversity_is_negative() {
        // Negative path_diversity (e.g. -1) cannot be represented as u32.
        // This indicates data corruption (e.g. an i32 underflow written to the DB).
        // The adapter must treat the snapshot as absent rather than clamping or
        // silently accepting the value — fail closed per "reject, don't sanitize".
        let mut snapshot = base_snapshot();
        snapshot.path_diversity = Some(-1);
        let reader = make_reader(Some(snapshot));
        let result = reader.get_score(Uuid::new_v4(), None).await.unwrap();
        assert!(
            result.is_none(),
            "negative path_diversity must map to no score (data corruption), not clamped"
        );
    }

    #[tokio::test]
    async fn get_score_maps_null_path_diversity_as_zero() {
        // NULL path_diversity is treated as zero (no contributing paths observed yet),
        // which is a valid state — the snapshot is still returned.
        let mut snapshot = base_snapshot();
        snapshot.path_diversity = None;
        let reader = make_reader(Some(snapshot));
        let result = reader.get_score(Uuid::new_v4(), None).await.unwrap();
        let score = result.expect("NULL path_diversity should still yield a score");
        assert_eq!(score.path_diversity, 0);
    }

    #[tokio::test]
    async fn get_score_maps_null_eigenvector_centrality_as_zero() {
        // NULL eigenvector_centrality is treated as 0.0 (supplemental metric not yet
        // computed), which is a valid state — the snapshot is still returned.
        let mut snapshot = base_snapshot();
        snapshot.eigenvector_centrality = None;
        let reader = make_reader(Some(snapshot));
        let result = reader.get_score(Uuid::new_v4(), None).await.unwrap();
        let score = result.expect("NULL eigenvector_centrality should still yield a score");
        assert!(
            score.eigenvector_centrality.abs() < f64::EPSILON,
            "NULL eigenvector_centrality must map to 0.0"
        );
    }

    #[tokio::test]
    async fn get_score_returns_none_when_eigenvector_centrality_is_negative() {
        // Negative eigenvector_centrality cannot arise from a correct computation
        // (centrality values are always >= 0). Treating it as "no score" follows the
        // same fail-closed approach applied to negative trust_distance and
        // path_diversity.
        let mut snapshot = base_snapshot();
        snapshot.eigenvector_centrality = Some(-0.1);
        let reader = make_reader(Some(snapshot));
        let result = reader.get_score(Uuid::new_v4(), None).await.unwrap();
        assert!(
            result.is_none(),
            "negative eigenvector_centrality must map to no score (data corruption), not a negative centrality"
        );
    }

    #[tokio::test]
    async fn get_score_returns_none_when_eigenvector_centrality_is_nan() {
        // NaN eigenvector_centrality must be rejected — it is not < 0.0, so without an
        // explicit is_finite() check it would silently propagate to the caller as
        // f64::NAN, which is nonsensical as a centrality value.
        let mut snapshot = base_snapshot();
        snapshot.eigenvector_centrality = Some(f32::NAN);
        let reader = make_reader(Some(snapshot));
        let result = reader.get_score(Uuid::new_v4(), None).await.unwrap();
        assert!(
            result.is_none(),
            "NaN eigenvector_centrality must map to no score (data corruption), not NaN"
        );
    }

    #[tokio::test]
    async fn get_score_returns_none_when_eigenvector_centrality_is_infinite() {
        // INFINITY eigenvector_centrality similarly cannot arise from a correct
        // computation (centrality values are always finite) and must be treated as
        // data corruption. The is_finite() guard catches both NaN and INFINITY;
        // this test verifies the INFINITY case explicitly, matching the pattern
        // for trust_distance which has separate NaN and INFINITY tests.
        let mut snapshot = base_snapshot();
        snapshot.eigenvector_centrality = Some(f32::INFINITY);
        let reader = make_reader(Some(snapshot));
        let result = reader.get_score(Uuid::new_v4(), None).await.unwrap();
        assert!(
            result.is_none(),
            "INFINITY eigenvector_centrality must map to no score (data corruption), not INFINITY"
        );
    }

    // ─── has_endorsement ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn has_endorsement_returns_true_when_repo_returns_true() {
        let reader = make_endorsement_reader(Ok(true));
        let result = reader
            .has_endorsement(Uuid::new_v4(), "trust", &[Uuid::new_v4()])
            .await
            .unwrap();
        assert!(result, "expected has_endorsement to return true");
    }

    #[tokio::test]
    async fn has_endorsement_returns_false_when_repo_returns_false() {
        let reader = make_endorsement_reader(Ok(false));
        let result = reader
            .has_endorsement(Uuid::new_v4(), "trust", &[Uuid::new_v4()])
            .await
            .unwrap();
        assert!(!result, "expected has_endorsement to return false");
    }

    #[tokio::test]
    async fn has_endorsement_propagates_repo_error() {
        let reader =
            make_endorsement_reader(Err(TrustRepoError::Database(sqlx::Error::RowNotFound)));
        let result = reader
            .has_endorsement(Uuid::new_v4(), "trust", &[Uuid::new_v4()])
            .await;
        assert!(
            result.is_err(),
            "expected has_endorsement to propagate the repo error"
        );
    }
}
