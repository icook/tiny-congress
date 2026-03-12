//! Trust graph computation — distance (CTE) and path diversity.

use std::collections::HashMap;

use sqlx::PgPool;
use uuid::Uuid;

use crate::trust::repo::TrustRepo;

/// A computed trust score for a single user, relative to an anchor.
#[derive(Debug, Clone)]
pub struct ComputedScore {
    pub user_id: Uuid,
    /// Minimum weighted hop-count distance from the anchor. `None` if unreachable.
    pub trust_distance: Option<f32>,
    /// Approximate path diversity (count of distinct reachable direct endorsers).
    pub path_diversity: i32,
}

/// Intermediate row type for sqlx deserialization of the distance CTE.
#[derive(sqlx::FromRow)]
struct DistanceRow {
    user_id: Uuid,
    trust_distance: f32,
}

/// Intermediate row type for diversity query result.
#[derive(sqlx::FromRow)]
struct DiversityRow {
    user_id: Uuid,
    path_diversity: i32,
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
        e.subject_id                        AS user_id,
        (1.0 / e.weight)::real              AS distance,
        ARRAY[e.endorser_id, e.subject_id]  AS path
    FROM reputation__endorsements e
    WHERE e.endorser_id = $1
      AND e.revoked_at IS NULL
      AND e.endorser_id IS NOT NULL

    UNION ALL

    -- Recursive: traverse outward, avoiding cycles
    SELECT
        e.subject_id,
        (tg.distance + 1.0 / e.weight)::real,
        tg.path || e.subject_id
    FROM reputation__endorsements e
    JOIN trust_graph tg ON e.endorser_id = tg.user_id
    WHERE tg.distance < 10.0
      AND NOT (e.subject_id = ANY(tg.path))
      AND e.revoked_at IS NULL
      AND e.endorser_id IS NOT NULL
)
SELECT user_id, MIN(distance) AS trust_distance
FROM trust_graph
GROUP BY user_id
            ",
        )
        .bind(anchor_id)
        .fetch_all(&self.pool)
        .await?;

        let mut scores: Vec<ComputedScore> = rows
            .into_iter()
            .map(|r| ComputedScore {
                user_id: r.user_id,
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
                user_id: anchor_id,
                trust_distance: Some(0.0),
                path_diversity: 0,
            },
        );

        Ok(scores)
    }

    /// Compute approximate path diversity for each user reachable from `anchor_id`.
    ///
    /// The approximation counts, for each target user, the number of distinct
    /// active endorsers that are themselves reachable from the anchor (including
    /// the anchor itself). This captures the key sybil-resistance property at
    /// demo scale: a hub-and-spoke attacker who endorses N users through a single
    /// node gives those users diversity=1, while a user endorsed by K independent
    /// reachable nodes gets diversity=K.
    ///
    /// # Errors
    ///
    /// Returns a [`sqlx::Error`] if any database query fails.
    pub async fn compute_diversity_from(
        &self,
        anchor_id: Uuid,
    ) -> Result<Vec<(Uuid, i32)>, sqlx::Error> {
        // Step 1: collect all reachable user IDs (including the anchor itself).
        let distances = self.compute_distances_from(anchor_id).await?;
        let mut reachable: Vec<Uuid> = distances.iter().map(|s| s.user_id).collect();
        reachable.push(anchor_id);

        // Step 2: for each target user, count distinct active endorsers in the reachable set.
        let rows: Vec<DiversityRow> = sqlx::query_as(
            r"
SELECT
    e.subject_id                        AS user_id,
    COUNT(DISTINCT e.endorser_id)::int  AS path_diversity
FROM reputation__endorsements e
WHERE e.revoked_at IS NULL
  AND e.endorser_id IS NOT NULL
  AND e.endorser_id = ANY($1)
GROUP BY e.subject_id
            ",
        )
        .bind(&reachable)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| (r.user_id, r.path_diversity))
            .collect())
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
    ) -> Result<usize, anyhow::Error> {
        let distances = self.compute_distances_from(anchor_id).await?;
        let diversities: HashMap<Uuid, i32> = self
            .compute_diversity_from(anchor_id)
            .await?
            .into_iter()
            .collect();

        let count = distances.len();
        for score in &distances {
            // The anchor is the root of trust — its diversity is not meaningful
            // in the endorser-count sense, so we pin it high to avoid it being
            // flagged as low-diversity.
            let diversity = if score.user_id == anchor_id {
                i32::MAX
            } else {
                diversities.get(&score.user_id).copied().unwrap_or(0)
            };
            trust_repo
                .upsert_score(
                    score.user_id,
                    Some(anchor_id),
                    score.trust_distance,
                    Some(diversity),
                    None,
                )
                .await
                .map_err(|e| anyhow::anyhow!("upsert_score failed: {e}"))?;
        }
        Ok(count)
    }

    /// Compute global eigenvector centrality across the entire graph.
    ///
    /// Stubbed for post-demo implementation — global eigenvector centrality
    /// requires iterative power-method convergence and is not needed at demo scale.
    ///
    /// # Errors
    ///
    /// Always returns `Ok(0)` in the current stub implementation.
    pub fn recompute_global(&self, _trust_repo: &dyn TrustRepo) -> Result<usize, anyhow::Error> {
        Ok(0)
    }
}
