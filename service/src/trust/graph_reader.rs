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
                        trust_distance: f64::from(trust_distance_raw),
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
    use crate::trust::repo::{
        ActionRecord, DenouncementRecord, DenouncementWithUsername, InfluenceRecord, InviteRecord,
        ScoreSnapshot, TrustRepo, TrustRepoError,
    };
    use crate::trust::service::ActionType;
    use crate::trust::weight::{DeliveryMethod, RelationshipDepth};
    use async_trait::async_trait;

    struct FixedScoreRepo(Option<ScoreSnapshot>);

    #[async_trait]
    impl TrustRepo for FixedScoreRepo {
        async fn get_score(
            &self,
            _: Uuid,
            _: Option<Uuid>,
        ) -> Result<Option<ScoreSnapshot>, TrustRepoError> {
            Ok(self.0.clone())
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
        async fn has_identity_endorsement(
            &self,
            _: Uuid,
            _: &[Uuid],
            _: &str,
        ) -> Result<bool, TrustRepoError> {
            unimplemented!()
        }
    }

    fn make_reader(snapshot: Option<ScoreSnapshot>) -> TrustRepoGraphReader {
        TrustRepoGraphReader::new(Arc::new(FixedScoreRepo(snapshot)))
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
}
