//! Trust graph reading interface for room engine plugins.
//!
//! Engine plugins query the trust graph to evaluate eligibility constraints
//! (e.g. minimum trust distance, endorsement requirements). This module
//! defines the read-only interface they use — the concrete implementation
//! lives in the service layer.

use uuid::Uuid;

/// A point-in-time snapshot of trust metrics for a subject relative to an anchor.
///
/// Returned by [`TrustGraphReader::get_score`]. Each field corresponds to a
/// metric that eligibility constraints can reference:
///
/// - `trust_distance` — shortest weighted path length from anchor to subject
///   (lower is closer / more trusted).
/// - `path_diversity` — number of independent paths connecting the anchor to
///   the subject (higher means harder to fake via a single Sybil cluster).
/// - `eigenvector_centrality` — the subject's centrality in the global trust
///   graph (range 0.0..=1.0, higher means more broadly trusted).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TrustScoreSnapshot {
    /// Shortest weighted path length from anchor to subject.
    pub trust_distance: f64,
    /// Number of independent trust paths from anchor to subject.
    pub path_diversity: u32,
    /// Subject's eigenvector centrality in the global trust graph (0.0..=1.0).
    pub eigenvector_centrality: f64,
}

/// Read-only interface to the trust graph.
///
/// Room engine plugins use this trait to evaluate trust-based eligibility
/// constraints without coupling to the concrete trust engine implementation.
///
/// # Errors
///
/// Methods return `anyhow::Error` for infrastructure failures (database
/// unavailable, graph not yet computed, etc.). A missing score is represented
/// as `Ok(None)`, not as an error.
#[async_trait::async_trait]
pub trait TrustGraphReader: Send + Sync {
    /// Returns the composite trust score for `subject`, optionally relative to
    /// a specific `anchor` identity. When `anchor` is `None`, the
    /// implementation should use the community-wide default anchor set.
    ///
    /// Returns `Ok(None)` if no score exists for the subject (e.g. unknown
    /// identity, graph not yet computed).
    ///
    /// # Errors
    ///
    /// Returns an error on infrastructure failures.
    async fn get_score(
        &self,
        subject: Uuid,
        anchor: Option<Uuid>,
    ) -> Result<Option<TrustScoreSnapshot>, anyhow::Error>;

    /// Checks whether `subject` holds an endorsement for the given `topic`
    /// issued by at least one of the provided `verifier_ids`.
    ///
    /// This is the building block for the `identity_verified` constraint — it
    /// checks whether any of the listed verifiers have endorsed the subject
    /// for the specified topic.
    ///
    /// # Errors
    ///
    /// Returns an error on infrastructure failures.
    async fn has_endorsement(
        &self,
        subject: Uuid,
        topic: &str,
        verifier_ids: &[Uuid],
    ) -> Result<bool, anyhow::Error>;
}
