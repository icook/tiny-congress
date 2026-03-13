//! Denouncement mechanism simulators.
//!
//! Each function applies a candidate denouncement mechanism to a graph,
//! re-runs the engine, and returns the updated report. These are test-level
//! simulations — no engine changes.

use sqlx::PgPool;
use uuid::Uuid;

use super::report::SimulationReport;
use super::GraphBuilder;

/// Mechanism 1: Edge removal.
///
/// Revokes all inbound edges to the target, re-runs engine + materialize.
pub async fn apply_edge_removal(
    g: &mut GraphBuilder,
    target: Uuid,
    anchor: Uuid,
    pool: &PgPool,
) -> SimulationReport {
    // Find and revoke all active inbound edges to target
    let inbound: Vec<(Uuid, Uuid)> = g
        .all_edges()
        .iter()
        .filter(|e| e.to == target && !e.revoked)
        .map(|e| (e.from, e.to))
        .collect();
    for (from, to) in inbound {
        g.revoke(from, to).await;
    }
    let report = SimulationReport::run(g, anchor).await;
    report.materialize(pool).await;
    report
}

/// Mechanism 2: Score penalty.
///
/// Runs engine + materialize normally, then directly modifies the target's
/// snapshot row (distance += penalty, diversity -= 1 clamped to 0).
pub async fn apply_score_penalty(
    g: &GraphBuilder,
    target: Uuid,
    anchor: Uuid,
    pool: &PgPool,
    distance_penalty: f32,
    diversity_penalty: i32,
) -> SimulationReport {
    let mut report = SimulationReport::run(g, anchor).await;
    report.materialize(pool).await;
    // Directly mutate the snapshot
    sqlx::query(
        "UPDATE trust__score_snapshots \
         SET trust_distance = COALESCE(trust_distance, 0) + $1, \
             path_diversity = GREATEST(COALESCE(path_diversity, 0) - $2, 0) \
         WHERE user_id = $3 AND context_user_id = $4",
    )
    .bind(distance_penalty)
    .bind(diversity_penalty)
    .bind(target)
    .bind(anchor)
    .execute(pool)
    .await
    .expect("score penalty UPDATE failed");
    report.refresh_from_snapshot(pool).await;
    report
}

/// Mechanism 3: Sponsorship cascade.
///
/// Revokes endorser→target edges AND applies score penalty to endorsers.
/// Re-runs engine + materialize.
pub async fn apply_sponsorship_cascade(
    g: &mut GraphBuilder,
    target: Uuid,
    anchor: Uuid,
    pool: &PgPool,
) -> SimulationReport {
    // Find endorsers of the target (active inbound edges)
    let endorsers: Vec<Uuid> = g
        .all_edges()
        .iter()
        .filter(|e| e.to == target && !e.revoked)
        .map(|e| e.from)
        .collect();
    // Revoke endorser→target edges
    for &endorser in &endorsers {
        g.revoke(endorser, target).await;
    }
    // Re-run engine with edges revoked
    let mut report = SimulationReport::run(g, anchor).await;
    report.materialize(pool).await;
    // Apply penalty to endorsers' snapshots. Lighter than the primary
    // score_penalty (3.0/1) because endorsers are collateral, not the target —
    // they vouched for a bad actor but aren't the bad actor themselves.
    for &endorser in &endorsers {
        sqlx::query(
            "UPDATE trust__score_snapshots \
             SET trust_distance = COALESCE(trust_distance, 0) + 2.0, \
                 path_diversity = GREATEST(COALESCE(path_diversity, 0) - 1, 0) \
             WHERE user_id = $1 AND context_user_id = $2",
        )
        .bind(endorser)
        .bind(anchor)
        .execute(pool)
        .await
        .expect("sponsorship penalty UPDATE failed");
    }
    report.refresh_from_snapshot(pool).await;
    report
}
