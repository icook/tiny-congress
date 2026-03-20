//! Trust graph computation — distance (CTE) and path diversity.

use std::collections::HashMap;

use sqlx::PgPool;
use uuid::Uuid;

use crate::trust::max_flow::FlowGraph;
use crate::trust::repo::{TrustRepo, TrustRepoError};

/// Errors returned by [`TrustEngine::recompute_from_anchor`].
#[derive(Debug, thiserror::Error)]
pub enum TrustEngineError {
    /// A database query failed during score computation.
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    /// Persisting a computed score failed.
    #[error("upsert_score failed: {0}")]
    UpsertScore(#[from] TrustRepoError),
}

/// A computed trust score for a single user, relative to an anchor.
#[derive(Debug, Clone)]
pub struct ComputedScore {
    pub account_id: Uuid,
    /// Minimum weighted hop-count distance from the anchor. `None` if unreachable.
    pub trust_distance: Option<f32>,
    /// Vertex connectivity (maximum number of internally node-disjoint paths) from the anchor.
    pub path_diversity: i32,
}

/// Intermediate row type for sqlx deserialization of the distance CTE.
#[derive(sqlx::FromRow)]
struct DistanceRow {
    account_id: Uuid,
    trust_distance: f32,
}

/// Intermediate row type for edge loading (diversity computation).
#[derive(sqlx::FromRow)]
struct EdgeRow {
    endorser_id: Uuid,
    subject_id: Uuid,
}

/// Computes and materializes trust scores from the endorsement graph.
pub struct TrustEngine {
    pool: PgPool,
}

impl TrustEngine {
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Compute the minimum weighted hop-count distance from `anchor_id` to every
    /// reachable user using a recursive CTE (TRD Section 3.1.1).
    ///
    /// A lower-weight edge is treated as a higher cost: an edge with `weight w`
    /// contributes `1.0 / w` to the running distance. The traversal stops when
    /// accumulated distance exceeds 10.0 or a cycle would be revisited.
    ///
    /// # Errors
    ///
    /// Returns a [`sqlx::Error`] if the database query fails.
    pub async fn compute_distances_from(
        &self,
        anchor_id: Uuid,
    ) -> Result<Vec<ComputedScore>, sqlx::Error> {
        let rows: Vec<DistanceRow> = sqlx::query_as(
            r"
WITH RECURSIVE trust_graph AS (
    -- Base: direct endorsements from the anchor
    SELECT
        e.subject_id                        AS account_id,
        (1.0 / e.weight)::real              AS distance,
        ARRAY[e.endorser_id, e.subject_id]  AS path
    FROM reputation__endorsements e
    WHERE e.endorser_id = $1
      AND e.revoked_at IS NULL
      AND e.endorser_id IS NOT NULL
      AND e.topic = 'trust'
      AND e.in_slot = true

    UNION ALL

    -- Recursive: traverse outward, avoiding cycles
    SELECT
        e.subject_id,
        (tg.distance + 1.0 / e.weight)::real,
        tg.path || e.subject_id
    FROM reputation__endorsements e
    JOIN trust_graph tg ON e.endorser_id = tg.account_id
    WHERE tg.distance < 10.0
      AND NOT (e.subject_id = ANY(tg.path))
      AND e.revoked_at IS NULL
      AND e.endorser_id IS NOT NULL
      AND e.topic = 'trust'
      AND e.in_slot = true
)
SELECT account_id, MIN(distance) AS trust_distance
FROM trust_graph
GROUP BY account_id
            ",
        )
        .bind(anchor_id)
        .fetch_all(&self.pool)
        .await?;

        let mut scores: Vec<ComputedScore> = rows
            .into_iter()
            .map(|r| ComputedScore {
                account_id: r.account_id,
                trust_distance: Some(r.trust_distance),
                path_diversity: 0,
            })
            .collect();

        // The anchor itself is the root of trust — distance 0 by definition.
        // The CTE only traverses outward edges, so it never produces a row for
        // the anchor. We inject it here so callers always see it.
        scores.insert(
            0,
            ComputedScore {
                account_id: anchor_id,
                trust_distance: Some(0.0),
                path_diversity: 0,
            },
        );

        Ok(scores)
    }

    /// Compute vertex connectivity (exact node-disjoint path count via Edmonds-Karp max-flow)
    /// for each user reachable from `anchor_id`.
    ///
    /// Loads the reachable subgraph into memory and runs max-flow on a vertex-split graph
    /// (Menger's theorem) to count the maximum number of internally node-disjoint paths
    /// from the anchor to each target. This is resistant to dense adversarial clusters:
    /// a fully-connected ring attached through a single bridge node scores diversity=1.
    ///
    /// # Errors
    ///
    /// Returns a [`sqlx::Error`] if any database query fails.
    pub async fn compute_diversity_from(
        &self,
        anchor_id: Uuid,
    ) -> Result<Vec<(Uuid, i32)>, sqlx::Error> {
        // Collect all reachable user IDs (the anchor is included in the distance results).
        let distances = self.compute_distances_from(anchor_id).await?;
        let reachable: Vec<Uuid> = distances.iter().map(|s| s.account_id).collect();
        self.diversity_from_reachable(anchor_id, &reachable).await
    }

    /// Compute vertex connectivity given a pre-computed set of reachable node IDs.
    ///
    /// Used by both [`compute_diversity_from`] (which computes the reachable set itself)
    /// and [`recompute_from_anchor`] (which passes the already-computed set to avoid a
    /// redundant database query).
    async fn diversity_from_reachable(
        &self,
        anchor_id: Uuid,
        reachable: &[Uuid],
    ) -> Result<Vec<(Uuid, i32)>, sqlx::Error> {
        // Step 1: build a stable index map Uuid → usize for the reachable set.
        let index_map: HashMap<Uuid, usize> = reachable
            .iter()
            .enumerate()
            .map(|(i, &id)| (id, i))
            .collect();
        let n = reachable.len();

        // Step 2: load all edges within the reachable subgraph.
        let edges: Vec<EdgeRow> = sqlx::query_as(
            r"
SELECT endorser_id, subject_id
FROM reputation__endorsements
WHERE revoked_at IS NULL
  AND endorser_id IS NOT NULL
  AND topic = 'trust'
  AND in_slot = true
  AND endorser_id = ANY($1)
  AND subject_id = ANY($1)
            ",
        )
        .bind(reachable)
        .fetch_all(&self.pool)
        .await?;

        // Step 3: build the FlowGraph from the edge list.
        let mut graph = FlowGraph::new(n);
        for edge in &edges {
            if let (Some(&from), Some(&to)) = (
                index_map.get(&edge.endorser_id),
                index_map.get(&edge.subject_id),
            ) {
                graph.add_edge(from, to);
            }
        }

        // Step 4: the anchor index in the reachable list (always index 0 since
        // compute_distances_from inserts it first).
        let anchor_index = index_map.get(&anchor_id).copied().unwrap_or(0);

        // Step 5: for each reachable node except the anchor, compute vertex connectivity.
        let results = reachable
            .iter()
            .enumerate()
            .filter(|(_, &id)| id != anchor_id)
            .map(|(node_index, &account_id)| {
                let connectivity = graph.vertex_connectivity(anchor_index, node_index);
                (account_id, connectivity)
            })
            .collect();

        Ok(results)
    }

    /// Run both computations and write the results to `trust__score_snapshots`.
    ///
    /// Returns the number of user scores written.
    ///
    /// # Errors
    ///
    /// Returns an error if any database query or upsert fails.
    pub async fn recompute_from_anchor(
        &self,
        anchor_id: Uuid,
        trust_repo: &dyn TrustRepo,
    ) -> Result<usize, TrustEngineError> {
        let distances = self.compute_distances_from(anchor_id).await?;
        let reachable: Vec<Uuid> = distances.iter().map(|s| s.account_id).collect();
        let diversities: HashMap<Uuid, i32> = self
            .diversity_from_reachable(anchor_id, &reachable)
            .await?
            .into_iter()
            .collect();

        let count = distances.len();
        for score in &distances {
            // The anchor is the root of trust — its diversity is not meaningful
            // in the endorser-count sense, so we pin it high to avoid it being
            // flagged as low-diversity.
            let diversity = if score.account_id == anchor_id {
                i32::MAX
            } else {
                diversities.get(&score.account_id).copied().unwrap_or(0)
            };
            trust_repo
                .upsert_score(
                    score.account_id,
                    Some(anchor_id),
                    score.trust_distance,
                    Some(diversity),
                    None,
                )
                .await?;
        }
        Ok(count)
    }
}
