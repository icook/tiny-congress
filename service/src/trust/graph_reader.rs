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
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        Ok(snapshot.map(|s| TrustScoreSnapshot {
            trust_distance: f64::from(s.trust_distance.unwrap_or(0.0)),
            path_diversity: u32::try_from(s.path_diversity.unwrap_or(0)).unwrap_or(0),
            eigenvector_centrality: f64::from(s.eigenvector_centrality.unwrap_or(0.0)),
        }))
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
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}
