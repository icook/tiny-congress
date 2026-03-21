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
                    Some(TrustScoreSnapshot {
                        trust_distance: f64::from(s.trust_distance.unwrap_or(0.0)),
                        path_diversity,
                        eigenvector_centrality: f64::from(s.eigenvector_centrality.unwrap_or(0.0)),
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
    use std::sync::Arc;

    use async_trait::async_trait;
    use uuid::Uuid;

    use crate::trust::repo::{
        ActionRecord, DenouncementRecord, DenouncementWithUsername, InfluenceRecord, InviteRecord,
        ScoreSnapshot, TrustRepo, TrustRepoError,
    };

    fn make_score(path_diversity: Option<i32>) -> ScoreSnapshot {
        ScoreSnapshot {
            user_id: Uuid::new_v4(),
            context_user_id: None,
            trust_distance: Some(1.5),
            path_diversity,
            eigenvector_centrality: Some(0.4),
            computed_at: chrono::Utc::now(),
        }
    }

    struct StubRepo {
        score: Option<ScoreSnapshot>,
    }

    #[async_trait]
    impl TrustRepo for StubRepo {
        async fn get_score(
            &self,
            _user_id: Uuid,
            _context_user_id: Option<Uuid>,
        ) -> Result<Option<ScoreSnapshot>, TrustRepoError> {
            Ok(self.score.clone())
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
            _: &str,
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
            _: &str,
            _: Option<&str>,
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
    async fn no_snapshot_returns_none() {
        let reader = TrustRepoGraphReader::new(Arc::new(StubRepo { score: None }));
        let result = reader.get_score(Uuid::new_v4(), None).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn valid_snapshot_maps_fields_correctly() {
        let score = make_score(Some(3));
        let reader = TrustRepoGraphReader::new(Arc::new(StubRepo { score: Some(score) }));
        let result = reader.get_score(Uuid::new_v4(), None).await.unwrap();
        let snapshot = result.expect("expected Some for valid score");
        assert_eq!(snapshot.path_diversity, 3);
        assert!((snapshot.trust_distance - 1.5).abs() < f64::EPSILON);
        assert!((snapshot.eigenvector_centrality - 0.4).abs() < 1e-6);
    }

    /// A negative path_diversity value indicates data corruption in the DB.
    /// The adapter treats this as "no score" (returns None) rather than
    /// propagating garbage data or returning an error.
    #[tokio::test]
    async fn negative_path_diversity_is_treated_as_no_score() {
        let score = make_score(Some(-1));
        let reader = TrustRepoGraphReader::new(Arc::new(StubRepo { score: Some(score) }));
        let result = reader.get_score(Uuid::new_v4(), None).await.unwrap();
        assert!(
            result.is_none(),
            "negative path_diversity should be treated as no score, not propagated"
        );
    }

    #[tokio::test]
    async fn null_path_diversity_defaults_to_zero() {
        let score = make_score(None);
        let reader = TrustRepoGraphReader::new(Arc::new(StubRepo { score: Some(score) }));
        let result = reader.get_score(Uuid::new_v4(), None).await.unwrap();
        let snapshot = result.expect("expected Some for score with null diversity");
        assert_eq!(snapshot.path_diversity, 0);
    }
}
