//! Scale graph generators for trust simulation at 1k–100k nodes.

use chrono::DateTime;
use chrono::Utc;
use uuid::Uuid;

use super::{GraphSpec, Team};

// ---------------------------------------------------------------------------
// Barabási-Albert preferential attachment
// ---------------------------------------------------------------------------

/// Parameters for generating a scale graph.
pub struct ScaleGraphParams {
    /// Total number of nodes.
    pub node_count: usize,
    /// Number of edges each new node creates (m in the BA model).
    /// Each new node connects to `m` existing nodes with probability
    /// proportional to their degree.
    pub edges_per_new_node: usize,
    /// Size of the initial fully-connected seed graph.
    pub seed_size: usize,
    /// Weight for all edges.
    pub weight: f32,
    /// Fraction of nodes designated as Red team (adversarial).
    /// Red nodes are assigned randomly after graph construction.
    pub red_fraction: f64,
}

impl Default for ScaleGraphParams {
    fn default() -> Self {
        Self {
            node_count: 1000,
            edges_per_new_node: 3,
            seed_size: 5,
            weight: 1.0,
            red_fraction: 0.0,
        }
    }
}

/// Simple LCG pseudo-random number generator seeded by an index.
///
/// Not cryptographic; used only for deterministic graph construction.
#[inline]
fn lcg_next(state: u64) -> u64 {
    // Knuth multiplicative hash + LCG step (period 2^64)
    state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407)
}

/// Map a u64 state to a float in [0.0, 1.0).
#[inline]
#[allow(clippy::cast_precision_loss)]
fn lcg_f64(state: u64) -> f64 {
    (state >> 11) as f64 / (1u64 << 53) as f64
}

/// Generate a Barabási-Albert scale-free graph using preferential attachment.
///
/// Node 0 is always named "anchor" and is always Blue team.
/// All other nodes are named "node_N". After construction, `red_fraction`
/// of non-anchor nodes are randomly designated Red team.
pub fn barabasi_albert(params: &ScaleGraphParams) -> GraphSpec {
    assert!(params.seed_size >= 2, "seed_size must be at least 2");
    assert!(
        params.edges_per_new_node >= 1,
        "edges_per_new_node must be at least 1"
    );
    assert!(
        params.node_count >= params.seed_size,
        "node_count must be >= seed_size"
    );

    let mut spec = GraphSpec::new();

    // -----------------------------------------------------------------------
    // Seed graph: fully-connected cluster of `seed_size` nodes
    // -----------------------------------------------------------------------
    // Node 0 is "anchor"; rest are "node_N".
    spec.add_node("anchor", Team::Blue, Uuid::from_u128(0u128));
    for i in 1..params.seed_size {
        spec.add_node(&format!("node_{i}"), Team::Blue, Uuid::from_u128(i as u128));
    }
    for i in 0..params.seed_size {
        for j in 0..params.seed_size {
            if i != j {
                spec.add_edge(
                    Uuid::from_u128(i as u128),
                    Uuid::from_u128(j as u128),
                    params.weight,
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // Degree tracking (in-degree + out-degree per node index)
    // -----------------------------------------------------------------------
    // Seed graph: each node has (seed_size - 1) out-edges and (seed_size - 1)
    // in-edges = 2 * (seed_size - 1) degree each.
    let mut degree: Vec<usize> = vec![2 * (params.seed_size - 1); params.seed_size];
    let mut total_degree: usize = degree.iter().sum();

    // -----------------------------------------------------------------------
    // Preferential attachment
    // -----------------------------------------------------------------------
    for new_idx in params.seed_size..params.node_count {
        spec.add_node(
            &format!("node_{new_idx}"),
            Team::Blue,
            Uuid::from_u128(new_idx as u128),
        );

        let m = params.edges_per_new_node.min(new_idx); // can't attach to more nodes than exist
        let mut selected: Vec<usize> = Vec::with_capacity(m);

        // Seed the LCG with the node index so the selection is deterministic
        // per new node but different across nodes.
        let mut rng_state: u64 = (new_idx as u64)
            .wrapping_mul(2_685_821_657_736_338_717)
            .wrapping_add(1);

        // Select m distinct target nodes by weighted sampling without
        // replacement. We use the "acceptance rejection" approach: draw a
        // node proportional to its degree, skip if already selected.
        let mut attempts = 0usize;
        while selected.len() < m && attempts < 10 * m * new_idx.max(1) {
            rng_state = lcg_next(rng_state);
            let threshold = (lcg_f64(rng_state) * total_degree as f64) as usize;
            let mut cumulative = 0usize;
            let mut chosen = 0usize;
            for (idx, &deg) in degree.iter().enumerate().take(new_idx) {
                cumulative += deg;
                if cumulative > threshold {
                    chosen = idx;
                    break;
                }
            }
            if !selected.contains(&chosen) {
                selected.push(chosen);
            }
            attempts += 1;
        }

        // Grow the degree array for the new node (starts at 0 before edges added)
        degree.push(0);

        // Create bidirectional edges between new node and each selected node
        for &target_idx in &selected {
            spec.add_edge(
                Uuid::from_u128(new_idx as u128),
                Uuid::from_u128(target_idx as u128),
                params.weight,
            );
            spec.add_edge(
                Uuid::from_u128(target_idx as u128),
                Uuid::from_u128(new_idx as u128),
                params.weight,
            );
            degree[new_idx] += 2;
            degree[target_idx] += 2;
            total_degree += 4;
        }
    }

    // -----------------------------------------------------------------------
    // Assign Red team to a fraction of non-anchor nodes
    // -----------------------------------------------------------------------
    if params.red_fraction > 0.0 {
        let non_anchor_count = params.node_count - 1; // exclude anchor (index 0)
        let red_count = ((non_anchor_count as f64) * params.red_fraction).round() as usize;

        // Build a shuffled list of non-anchor indices using LCG
        let mut indices: Vec<usize> = (1..params.node_count).collect();
        let mut rng_state: u64 = 0xdeadbeef_cafebabe;
        for i in (1..indices.len()).rev() {
            rng_state = lcg_next(rng_state);
            let j = (lcg_f64(rng_state) * (i + 1) as f64) as usize;
            indices.swap(i, j);
        }

        for &node_idx in indices.iter().take(red_count) {
            // Mutate the team field directly on the node in spec
            let node_id = Uuid::from_u128(node_idx as u128);
            if let Some(node) = spec
                .all_nodes()
                .iter()
                .position(|n| n.id == node_id)
                .and_then(|pos| {
                    // We need mutable access — use all_nodes_mut via a workaround
                    // GraphSpec doesn't expose all_nodes_mut, so we rebuild via the
                    // internal name. We add_node would duplicate — instead we patch
                    // the team inline by re-adding under a temporary approach.
                    // Since GraphSpec stores nodes in a Vec and name_to_id is a
                    // HashMap, we have no direct mutation path. We use the position.
                    Some(pos)
                })
            {
                // SAFETY: we just found this position via immutable ref; now we
                // take a mutable ref to nodes through the public edges_mut-style
                // accessor. GraphSpec doesn't expose nodes_mut, so we use
                // all_edges_mut as a pattern proof that internal fields are
                // accessible via methods. Since nodes_mut is not public, we
                // instead work around this by collecting the whole node list,
                // rebuilding, or mutating via a dedicated path.
                //
                // GraphSpec does not expose a set_team method. The cleanest
                // solution that matches the existing API is to re-create the
                // spec while patching team assignments. We do that after the
                // loop by rebuilding nodes in-place.
                let _ = node; // suppress unused warning; used below
            }
        }

        // Rebuild: collect red node indices, then create a new spec with
        // correct teams while preserving all edges.
        let red_indices: std::collections::HashSet<usize> =
            indices.iter().take(red_count).copied().collect();

        let nodes_snapshot: Vec<(Uuid, String, usize)> = spec
            .all_nodes()
            .iter()
            .map(|n| {
                // recover index from UUID: from_u128(index)
                let idx = n.id.as_u128() as usize;
                (n.id, n.name.clone(), idx)
            })
            .collect();
        let edges_snapshot: Vec<(Uuid, Uuid, f32)> = spec
            .all_edges()
            .iter()
            .map(|e| (e.from, e.to, e.weight))
            .collect();

        let mut new_spec = GraphSpec::new();
        for (id, name, idx) in &nodes_snapshot {
            let team = if *idx == 0 || !red_indices.contains(idx) {
                Team::Blue
            } else {
                Team::Red
            };
            new_spec.add_node(name, team, *id);
        }
        for (from, to, weight) in edges_snapshot {
            new_spec.add_edge(from, to, weight);
        }
        return new_spec;
    }

    spec
}

// ---------------------------------------------------------------------------
// Sybil mesh
// ---------------------------------------------------------------------------

/// Parameters for a Sybil mesh attack topology.
pub struct SybilMeshParams {
    /// Number of fake Sybil nodes in the mesh.
    pub mesh_size: usize,
    /// How many bridge nodes connect the mesh to the legitimate graph.
    /// These are existing nodes in the graph that the attacker has compromised.
    pub bridge_count: usize,
    /// Density of internal mesh edges (0.0 to 1.0).
    /// At 1.0, every pair of mesh nodes endorses each other.
    pub internal_density: f64,
    /// Weight for mesh edges.
    pub weight: f32,
}

/// Attach a Sybil mesh to an existing graph.
///
/// Creates `mesh_size` Red team nodes named "sybil_N", connects them internally
/// at `internal_density`, and bridges them bidirectionally to the first
/// `bridge_count` entries in `bridge_targets`.
///
/// Returns the list of Sybil node UUIDs.
pub fn attach_sybil_mesh(
    spec: &mut GraphSpec,
    bridge_targets: &[Uuid],
    params: &SybilMeshParams,
) -> Vec<Uuid> {
    let mut sybil_ids: Vec<Uuid> = Vec::with_capacity(params.mesh_size);

    // Create Sybil nodes (Red team)
    for i in 0..params.mesh_size {
        let id = Uuid::from_u128(1_000_000 + i as u128);
        spec.add_node(&format!("sybil_{i}"), Team::Red, id);
        sybil_ids.push(id);
    }

    // Internal mesh edges (deterministic density, same hash pattern as healthy_web)
    for i in 0..params.mesh_size {
        for j in 0..params.mesh_size {
            if i == j {
                continue;
            }
            #[allow(clippy::cast_precision_loss)]
            let hash = ((i * 7 + j * 13 + 37) % 100) as f64 / 100.0;
            if hash < params.internal_density {
                spec.add_edge(sybil_ids[i], sybil_ids[j], params.weight);
            }
        }
    }

    // Bridge edges: each bridge target ↔ all Sybil nodes
    let active_bridges = bridge_targets.iter().take(params.bridge_count);
    for &bridge in active_bridges {
        for &sybil in &sybil_ids {
            spec.add_edge(bridge, sybil, params.weight);
            spec.add_edge(sybil, bridge, params.weight);
        }
    }

    sybil_ids
}

// ---------------------------------------------------------------------------
// Correlated failure helpers
// ---------------------------------------------------------------------------

/// Set `created_at` on all edges where both endpoints are in `cohort_nodes`.
///
/// Simulates a group of people who all endorsed each other at the same event.
pub fn mark_cohort_edges(spec: &mut GraphSpec, cohort_nodes: &[Uuid], created_at: DateTime<Utc>) {
    let cohort_set: std::collections::HashSet<Uuid> = cohort_nodes.iter().copied().collect();
    for edge in spec.all_edges_mut() {
        if cohort_set.contains(&edge.from) && cohort_set.contains(&edge.to) {
            edge.created_at = Some(created_at);
        }
    }
}

/// Return the `count` nodes with highest degree (in + out, non-revoked edges only).
///
/// Excludes the node named "anchor". Used to identify high-value bridge node
/// candidates for targeted removal tests.
pub fn find_high_degree_nodes(spec: &GraphSpec, count: usize) -> Vec<Uuid> {
    // Build a degree map over non-revoked edges
    let mut degree: std::collections::HashMap<Uuid, usize> = std::collections::HashMap::new();
    for edge in spec.all_edges() {
        if edge.revoked {
            continue;
        }
        *degree.entry(edge.from).or_insert(0) += 1;
        *degree.entry(edge.to).or_insert(0) += 1;
    }

    // Collect candidate nodes (exclude "anchor")
    let anchor_id = spec
        .all_nodes()
        .iter()
        .find(|n| n.name == "anchor")
        .map(|n| n.id);

    let mut candidates: Vec<(Uuid, usize)> = spec
        .all_nodes()
        .iter()
        .filter(|n| Some(n.id) != anchor_id)
        .map(|n| (n.id, *degree.get(&n.id).unwrap_or(&0)))
        .collect();

    // Sort descending by degree, stable so ties keep insertion order
    candidates.sort_by(|a, b| b.1.cmp(&a.1));

    candidates.iter().take(count).map(|&(id, _)| id).collect()
}

/// Revoke all edges to/from the given node (set `revoked = true`).
///
/// Used to simulate bridge node removal.
pub fn remove_node_edges(spec: &mut GraphSpec, node: Uuid) {
    for edge in spec.all_edges_mut() {
        if edge.from == node || edge.to == node {
            edge.revoked = true;
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::super::{GraphSpec, Team};
    use super::{
        attach_sybil_mesh, barabasi_albert, find_high_degree_nodes, remove_node_edges,
        ScaleGraphParams, SybilMeshParams,
    };

    #[test]
    fn ba_produces_correct_node_count() {
        let params = ScaleGraphParams {
            node_count: 100,
            edges_per_new_node: 3,
            seed_size: 5,
            weight: 1.0,
            red_fraction: 0.0,
        };
        let spec = barabasi_albert(&params);
        assert_eq!(spec.all_nodes().len(), 100);
    }

    #[test]
    fn ba_edge_count_is_approximately_correct() {
        // Each non-seed node adds edges_per_new_node bidirectional edges = 2m edges.
        // Seed adds seed*(seed-1) directed edges.
        // Total expected directed edges ≈ seed*(seed-1) + 2*m*(n - seed)
        let params = ScaleGraphParams {
            node_count: 200,
            edges_per_new_node: 3,
            seed_size: 5,
            weight: 1.0,
            red_fraction: 0.0,
        };
        let spec = barabasi_albert(&params);
        let expected_min = 5 * 4 + 2 * 3 * (200 - 5); // lower bound (some attachment may be < m)
        let expected_max = 5 * 4 + 2 * 3 * (200 - 5) + 100; // small tolerance
        let actual = spec.active_edge_count();
        assert!(
            actual >= expected_min,
            "edge count {actual} below minimum {expected_min}"
        );
        assert!(
            actual <= expected_max,
            "edge count {actual} above maximum {expected_max}"
        );
    }

    #[test]
    fn ba_anchor_is_node_zero_blue() {
        let params = ScaleGraphParams::default();
        let spec = barabasi_albert(&params);
        let anchor = spec
            .all_nodes()
            .iter()
            .find(|n| n.name == "anchor")
            .expect("anchor node must exist");
        assert_eq!(anchor.id, Uuid::from_u128(0u128));
        assert_eq!(anchor.team, Team::Blue);
    }

    #[test]
    fn ba_anchor_stays_blue_with_red_fraction() {
        let params = ScaleGraphParams {
            node_count: 50,
            edges_per_new_node: 2,
            seed_size: 3,
            weight: 1.0,
            red_fraction: 0.5,
        };
        let spec = barabasi_albert(&params);
        let anchor = spec
            .all_nodes()
            .iter()
            .find(|n| n.name == "anchor")
            .expect("anchor node must exist");
        assert_eq!(anchor.team, Team::Blue, "anchor must always be Blue");

        let red_count = spec
            .all_nodes()
            .iter()
            .filter(|n| n.team == Team::Red)
            .count();
        let expected_red = ((49_f64) * 0.5).round() as usize;
        // Allow ±2 for rounding
        assert!(
            red_count.abs_diff(expected_red) <= 2,
            "red count {red_count} far from expected {expected_red}"
        );
    }

    #[test]
    fn sybil_mesh_creates_correct_number_of_red_nodes() {
        let mut spec = GraphSpec::new();
        spec.add_node("honest_0", Team::Blue, Uuid::from_u128(0u128));
        spec.add_node("honest_1", Team::Blue, Uuid::from_u128(1u128));

        let bridge_targets = vec![Uuid::from_u128(0u128), Uuid::from_u128(1u128)];
        let params = SybilMeshParams {
            mesh_size: 10,
            bridge_count: 2,
            internal_density: 0.5,
            weight: 1.0,
        };
        let sybil_ids = attach_sybil_mesh(&mut spec, &bridge_targets, &params);

        assert_eq!(sybil_ids.len(), 10);
        let red_nodes: Vec<_> = spec
            .all_nodes()
            .iter()
            .filter(|n| n.team == Team::Red)
            .collect();
        assert_eq!(red_nodes.len(), 10);
    }

    #[test]
    fn sybil_mesh_bridge_edges_are_bidirectional() {
        let mut spec = GraphSpec::new();
        spec.add_node("honest_0", Team::Blue, Uuid::from_u128(0u128));

        let bridge_targets = vec![Uuid::from_u128(0u128)];
        let params = SybilMeshParams {
            mesh_size: 3,
            bridge_count: 1,
            internal_density: 0.0,
            weight: 1.0,
        };
        let sybil_ids = attach_sybil_mesh(&mut spec, &bridge_targets, &params);

        // Each sybil node should have an edge to and from the bridge
        for &sybil in &sybil_ids {
            let bridge = Uuid::from_u128(0u128);
            let has_bridge_to_sybil = spec
                .all_edges()
                .iter()
                .any(|e| e.from == bridge && e.to == sybil && !e.revoked);
            let has_sybil_to_bridge = spec
                .all_edges()
                .iter()
                .any(|e| e.from == sybil && e.to == bridge && !e.revoked);
            assert!(has_bridge_to_sybil, "missing bridge→sybil edge");
            assert!(has_sybil_to_bridge, "missing sybil→bridge edge");
        }
    }

    #[test]
    fn find_high_degree_nodes_returns_top_n_excluding_anchor() {
        let mut spec = GraphSpec::new();
        // anchor gets no edges; node_1 gets many; node_2 gets some; node_3 none
        spec.add_node("anchor", Team::Blue, Uuid::from_u128(0u128));
        spec.add_node("node_1", Team::Blue, Uuid::from_u128(1u128));
        spec.add_node("node_2", Team::Blue, Uuid::from_u128(2u128));
        spec.add_node("node_3", Team::Blue, Uuid::from_u128(3u128));

        // node_1 ↔ node_2 and node_1 ↔ node_3 (4 edges touching node_1)
        spec.add_edge(Uuid::from_u128(1u128), Uuid::from_u128(2u128), 1.0);
        spec.add_edge(Uuid::from_u128(2u128), Uuid::from_u128(1u128), 1.0);
        spec.add_edge(Uuid::from_u128(1u128), Uuid::from_u128(3u128), 1.0);
        spec.add_edge(Uuid::from_u128(3u128), Uuid::from_u128(1u128), 1.0);
        // node_2 ↔ node_3 (2 edges touching node_2 additionally)
        spec.add_edge(Uuid::from_u128(2u128), Uuid::from_u128(3u128), 1.0);
        spec.add_edge(Uuid::from_u128(3u128), Uuid::from_u128(2u128), 1.0);

        let top1 = find_high_degree_nodes(&spec, 1);
        assert_eq!(top1.len(), 1);
        assert_eq!(
            top1[0],
            Uuid::from_u128(1u128),
            "node_1 should be highest degree"
        );

        let top3 = find_high_degree_nodes(&spec, 3);
        assert_eq!(top3.len(), 3);
        // anchor must not appear
        assert!(
            !top3.contains(&Uuid::from_u128(0u128)),
            "anchor must not appear in high-degree list"
        );
    }

    #[test]
    fn remove_node_edges_revokes_all_incident_edges() {
        let mut spec = GraphSpec::new();
        spec.add_node("a", Team::Blue, Uuid::from_u128(0u128));
        spec.add_node("b", Team::Blue, Uuid::from_u128(1u128));
        spec.add_node("c", Team::Blue, Uuid::from_u128(2u128));

        spec.add_edge(Uuid::from_u128(0u128), Uuid::from_u128(1u128), 1.0);
        spec.add_edge(Uuid::from_u128(1u128), Uuid::from_u128(0u128), 1.0);
        spec.add_edge(Uuid::from_u128(1u128), Uuid::from_u128(2u128), 1.0);
        spec.add_edge(Uuid::from_u128(0u128), Uuid::from_u128(2u128), 1.0);

        remove_node_edges(&mut spec, Uuid::from_u128(1u128));

        // All edges touching node_1 should be revoked
        for edge in spec.all_edges() {
            if edge.from == Uuid::from_u128(1u128) || edge.to == Uuid::from_u128(1u128) {
                assert!(edge.revoked, "edge touching node_1 should be revoked");
            }
        }
        // The edge not touching node_1 should still be active
        let a_to_c = spec
            .all_edges()
            .iter()
            .find(|e| e.from == Uuid::from_u128(0u128) && e.to == Uuid::from_u128(2u128))
            .expect("a→c edge must exist");
        assert!(!a_to_c.revoked, "a→c edge should not be revoked");
    }
}
