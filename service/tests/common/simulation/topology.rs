//! Parameterized topology generators for common attack patterns.
//!
//! Each generator creates named nodes and endorsement edges using `GraphBuilder`.
//! Nodes are named with a prefix for debugging (e.g., `red_hub`, `red_spoke_0`).

use uuid::Uuid;

use super::{GraphBuilder, Team};

/// Create a hub-and-spoke topology: one hub endorses N spokes.
///
/// Returns `(hub_id, spoke_ids)`.
pub async fn hub_and_spoke(
    g: &mut GraphBuilder,
    prefix: &str,
    team: Team,
    spoke_count: usize,
    weight: f32,
) -> (Uuid, Vec<Uuid>) {
    let hub = g.add_node(&format!("{prefix}_hub"), team).await;
    let mut spokes = Vec::with_capacity(spoke_count);
    for i in 0..spoke_count {
        let spoke = g.add_node(&format!("{prefix}_spoke_{i}"), team).await;
        g.endorse(hub, spoke, weight).await;
        spokes.push(spoke);
    }
    (hub, spokes)
}

/// Create a linear chain of endorsements.
///
/// Returns node IDs in chain order. First node is the chain head;
/// each node endorses the next.
pub async fn chain(
    g: &mut GraphBuilder,
    prefix: &str,
    team: Team,
    length: usize,
    weight: f32,
) -> Vec<Uuid> {
    let mut nodes = Vec::with_capacity(length);
    for i in 0..length {
        let node = g.add_node(&format!("{prefix}_chain_{i}"), team).await;
        if let Some(&prev) = nodes.last() {
            g.endorse(prev, node, weight).await;
        }
        nodes.push(node);
    }
    nodes
}

/// Create a colluding ring: each node endorses the next, last endorses first.
///
/// Returns node IDs in ring order.
pub async fn colluding_ring(
    g: &mut GraphBuilder,
    prefix: &str,
    team: Team,
    size: usize,
    weight: f32,
) -> Vec<Uuid> {
    assert!(size >= 2, "Ring must have at least 2 nodes");
    let mut nodes = Vec::with_capacity(size);
    for i in 0..size {
        let node = g.add_node(&format!("{prefix}_ring_{i}"), team).await;
        nodes.push(node);
    }
    for i in 0..size {
        let next = (i + 1) % size;
        g.endorse(nodes[i], nodes[next], weight).await;
    }
    nodes
}

/// Create a fully connected cluster: every node endorses every other node.
///
/// Returns all node IDs.
pub async fn fully_connected_cluster(
    g: &mut GraphBuilder,
    prefix: &str,
    team: Team,
    size: usize,
    weight: f32,
) -> Vec<Uuid> {
    let mut nodes = Vec::with_capacity(size);
    for i in 0..size {
        let node = g.add_node(&format!("{prefix}_cluster_{i}"), team).await;
        nodes.push(node);
    }
    for i in 0..size {
        for j in 0..size {
            if i != j {
                g.endorse(nodes[i], nodes[j], weight).await;
            }
        }
    }
    nodes
}

/// Create a hub-and-spoke where edges have staggered creation times.
///
/// The hub's first spoke edge is created at `base_time`. Each subsequent spoke's
/// edge is progressively newer: `spoke_0` at `base_time`, `spoke_1` at
/// `base_time + interval`, etc.
///
/// Returns `(hub_id, spoke_ids)`.
pub async fn hub_and_spoke_temporal(
    g: &mut GraphBuilder,
    prefix: &str,
    team: Team,
    spoke_count: usize,
    weight: f32,
    base_time: chrono::DateTime<chrono::Utc>,
    interval: chrono::Duration,
) -> (Uuid, Vec<Uuid>) {
    let hub = g.add_node(&format!("{prefix}_hub"), team).await;
    let mut spokes = Vec::with_capacity(spoke_count);
    for i in 0..spoke_count {
        let spoke = g.add_node(&format!("{prefix}_spoke_{i}"), team).await;
        g.endorse(hub, spoke, weight).await;
        // Set the creation time on the edge we just added
        if let Some(edge) = g.spec_mut().all_edges_mut().last_mut() {
            let step = i32::try_from(i).unwrap_or(i32::MAX);
            edge.created_at = Some(base_time + interval * step);
        }
        spokes.push(spoke);
    }
    (hub, spokes)
}

/// Create a healthy web: nodes with deterministic interconnections.
///
/// `density` is the proportion (0.0-1.0) of possible directed edges to create.
/// Edge selection is deterministic based on node indices for reproducibility.
///
/// Returns all node IDs.
pub async fn healthy_web(
    g: &mut GraphBuilder,
    prefix: &str,
    team: Team,
    size: usize,
    density: f64,
    weight: f32,
) -> Vec<Uuid> {
    let mut nodes = Vec::with_capacity(size);
    for i in 0..size {
        let node = g.add_node(&format!("{prefix}_web_{i}"), team).await;
        nodes.push(node);
    }
    for i in 0..size {
        for j in 0..size {
            if i == j {
                continue;
            }
            // Deterministic hash to decide if edge exists
            #[allow(clippy::cast_precision_loss)]
            let hash = ((i * 7 + j * 13 + 37) % 100) as f64 / 100.0;
            if hash < density {
                g.endorse(nodes[i], nodes[j], weight).await;
            }
        }
    }
    nodes
}
