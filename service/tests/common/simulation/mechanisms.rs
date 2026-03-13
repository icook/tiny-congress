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

/// Mechanism 4: Denouncer-only edge revocation (ADR-024).
///
/// Only revokes the denouncer→target edge. The denouncer is saying "I no longer
/// vouch for this person." A single denouncement costs the target one path, not
/// all of them — proportionate and weaponization-resistant.
///
/// If no active edge exists from denouncer→target, this is a no-op —
/// a node cannot revoke an endorsement it never made.
pub async fn apply_denouncer_revocation(
    g: &mut GraphBuilder,
    denouncer: Uuid,
    target: Uuid,
    anchor: Uuid,
    pool: &PgPool,
) -> SimulationReport {
    // Check if an active edge exists from denouncer → target
    let has_edge = g
        .all_edges()
        .iter()
        .any(|e| e.from == denouncer && e.to == target && !e.revoked);
    if has_edge {
        g.revoke(denouncer, target).await;
    }
    let report = SimulationReport::run(g, anchor).await;
    report.materialize(pool).await;
    report
}

/// Mechanism 3b: Denouncer-only revocation + sponsorship cascade.
///
/// Revokes the denouncer→target edge, THEN applies cascade penalties to
/// remaining endorsers of the target. Combines the proportionality of
/// denouncer-only with the "risk of endorsement" consequence.
pub async fn apply_denouncer_revocation_with_cascade(
    g: &mut GraphBuilder,
    denouncer: Uuid,
    target: Uuid,
    anchor: Uuid,
    pool: &PgPool,
) -> SimulationReport {
    apply_denouncer_revocation_with_cascade_params(g, denouncer, target, anchor, pool, 2.0, 1).await
}

/// Mechanism 3b (parameterized): Denouncer revocation with configurable cascade.
///
/// Same as `apply_denouncer_revocation_with_cascade` but with configurable
/// penalty values for parameter sweeps.
pub async fn apply_denouncer_revocation_with_cascade_params(
    g: &mut GraphBuilder,
    denouncer: Uuid,
    target: Uuid,
    anchor: Uuid,
    pool: &PgPool,
    distance_penalty: f32,
    diversity_penalty: i32,
) -> SimulationReport {
    // Revoke denouncer→target edge
    g.revoke(denouncer, target).await;
    // Re-run engine with the edge revoked
    let mut report = SimulationReport::run(g, anchor).await;
    report.materialize(pool).await;
    // Find remaining endorsers of target (active inbound edges after revocation)
    let remaining_endorsers: Vec<Uuid> = g
        .all_edges()
        .iter()
        .filter(|e| e.to == target && !e.revoked)
        .map(|e| e.from)
        .collect();
    // Apply penalty to remaining endorsers' snapshots
    for &endorser in &remaining_endorsers {
        sqlx::query(
            "UPDATE trust__score_snapshots \
             SET trust_distance = COALESCE(trust_distance, 0) + $1, \
                 path_diversity = GREATEST(COALESCE(path_diversity, 0) - $2, 0) \
             WHERE user_id = $3 AND context_user_id = $4",
        )
        .bind(distance_penalty)
        .bind(diversity_penalty)
        .bind(endorser)
        .bind(anchor)
        .execute(pool)
        .await
        .expect("cascade penalty UPDATE failed");
    }
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
