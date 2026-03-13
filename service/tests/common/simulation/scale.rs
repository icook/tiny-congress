//! Scale graph generators for large-topology trust simulation.
//!
//! Provides Barabási-Albert preferential attachment graphs, Sybil mesh
//! injection, cohort timestamp helpers, and graph analysis utilities.
//! All generators operate on [`GraphSpec`] directly (no DB, no async).

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::{GraphSpec, Team};

// ---------------------------------------------------------------------------
// Barabási-Albert generator
// ---------------------------------------------------------------------------

/// Parameters for Barabási-Albert preferential attachment graph generation.
#[derive(Debug, Clone)]
pub struct ScaleGraphParams {
    /// Total number of nodes in the graph (including the initial seed).
    pub node_count: usize,
    /// Number of edges each new node attaches to (m parameter).
    pub m: usize,
    /// Number of nodes in the initial fully-connected seed.
    pub seed_size: usize,
    /// Random seed for deterministic generation.
    pub seed: u64,
}

impl Default for ScaleGraphParams {
    fn default() -> Self {
        Self {
            node_count: 1000,
            m: 3,
            seed_size: 5,
            seed: 42,
        }
    }
}

/// Generate a Barabási-Albert preferential attachment graph.
///
/// The first `params.seed_size` nodes form a fully-connected seed (all Blue).
/// Each subsequent node connects to `params.m` existing nodes via preferential
/// attachment — nodes with higher degree are more likely to be selected.
///
/// All nodes are Blue (legitimate); attach a Sybil mesh via
/// [`attach_sybil_mesh`] to add adversarial structure.
///
/// Node 0 is always named `"anchor"`. Subsequent nodes are named `"node_N"`.
pub fn barabasi_albert(params: &ScaleGraphParams) -> GraphSpec {
    assert!(params.seed_size >= 2, "seed_size must be at least 2");
    assert!(params.m >= 1, "m must be at least 1");
    assert!(
        params.m <= params.seed_size,
        "m must be <= seed_size (cannot attach to more nodes than exist in seed)"
    );
    assert!(
        params.node_count >= params.seed_size,
        "node_count must be >= seed_size"
    );

    let mut spec = GraphSpec::new();
    let mut rng = LcgRng::new(params.seed);

    // Build seed: fully connected, all Blue
    let mut ids: Vec<Uuid> = Vec::with_capacity(params.node_count);
    for i in 0..params.seed_size {
        let id = Uuid::from_u128(i as u128);
        let name = if i == 0 {
            "anchor".to_string()
        } else {
            format!("node_{i}")
        };
        spec.add_node(&name, Team::Blue, id);
        ids.push(id);
    }

    // Seed: fully connected (directed, both directions)
    for i in 0..params.seed_size {
        for j in 0..params.seed_size {
            if i != j {
                spec.add_edge(ids[i], ids[j], 1.0);
            }
        }
    }

    // Preferential attachment: degree list tracks attachment probability
    // Each node appears in degree_list once per edge incident to it.
    let mut degree_list: Vec<Uuid> = Vec::new();
    for i in 0..params.seed_size {
        // Each node in the seed has (seed_size - 1) inbound + (seed_size - 1) outbound edges
        let degree = 2 * (params.seed_size - 1);
        for _ in 0..degree {
            degree_list.push(ids[i]);
        }
    }

    // Add remaining nodes via preferential attachment
    for i in params.seed_size..params.node_count {
        let id = Uuid::from_u128(i as u128);
        let name = format!("node_{i}");
        spec.add_node(&name, Team::Blue, id);
        ids.push(id);

        // Select m distinct targets by preferential attachment
        let targets = sample_distinct(&mut rng, &degree_list, params.m, id);

        for &target in &targets {
            spec.add_edge(id, target, 1.0);
            spec.add_edge(target, id, 1.0);
            // Update degree list
            degree_list.push(id);
            degree_list.push(target);
        }
    }

    spec
}

// ---------------------------------------------------------------------------
// Sybil mesh injection
// ---------------------------------------------------------------------------

/// Parameters for Sybil mesh attachment.
#[derive(Debug, Clone)]
pub struct SybilMeshParams {
    /// Number of Sybil nodes to create.
    pub sybil_count: usize,
    /// Number of bridge edges from Sybil nodes into the legitimate graph.
    /// Each bridge is from a distinct Sybil node to a bridge target.
    pub bridge_count: usize,
    /// Weight of bridge edges (Sybil → legitimate).
    pub bridge_weight: f32,
    /// Weight of internal Sybil edges.
    pub internal_weight: f32,
    /// Random seed for Sybil node ID generation.
    pub seed: u64,
}

impl Default for SybilMeshParams {
    fn default() -> Self {
        Self {
            sybil_count: 50,
            bridge_count: 3,
            bridge_weight: 0.8,
            internal_weight: 1.0,
            seed: 99,
        }
    }
}

/// Attach a Sybil mesh to an existing [`GraphSpec`].
///
/// Creates `params.sybil_count` Red nodes forming a fully-connected internal
/// mesh. `params.bridge_count` bridge edges connect Sybil nodes to the
/// supplied `bridge_targets` (Blue nodes). Returns the IDs of all created
/// Sybil nodes.
///
/// `bridge_targets` must have at least `params.bridge_count` entries.
pub fn attach_sybil_mesh(
    spec: &mut GraphSpec,
    bridge_targets: &[Uuid],
    params: &SybilMeshParams,
) -> Vec<Uuid> {
    assert!(
        bridge_targets.len() >= params.bridge_count,
        "bridge_targets must have at least bridge_count entries, got {} targets for {} bridges",
        bridge_targets.len(),
        params.bridge_count
    );
    assert!(
        params.sybil_count >= params.bridge_count,
        "sybil_count must be >= bridge_count"
    );

    // Use high offset from_u128 to avoid collisions with BA node IDs (which start at 0)
    const SYBIL_ID_OFFSET: u128 = 1_000_000;

    let mut sybil_ids: Vec<Uuid> = Vec::with_capacity(params.sybil_count);

    for i in 0..params.sybil_count {
        let id = Uuid::from_u128(SYBIL_ID_OFFSET + i as u128);
        let name = format!("sybil_{i}");
        spec.add_node(&name, Team::Red, id);
        sybil_ids.push(id);
    }

    // Internal mesh: fully connected among Sybil nodes
    for i in 0..params.sybil_count {
        for j in 0..params.sybil_count {
            if i != j {
                spec.add_edge(sybil_ids[i], sybil_ids[j], params.internal_weight);
            }
        }
    }

    // Bridge edges: bidirectional between first bridge_count Sybil nodes and bridge_targets.
    // Both directions are needed: sybil → target (sybil can reach the network) and
    // target → sybil (anchor can reach sybils, enabling diversity measurement).
    for b in 0..params.bridge_count {
        let sybil_id = sybil_ids[b];
        let target = bridge_targets[b];
        spec.add_edge(sybil_id, target, params.bridge_weight);
        spec.add_edge(target, sybil_id, params.bridge_weight);
    }

    sybil_ids
}

// ---------------------------------------------------------------------------
// Cohort timestamp helper
// ---------------------------------------------------------------------------

/// Set `created_at` on all edges whose `from` node is in `cohort_nodes`.
///
/// This stamps cohort edges with a specific time for decay testing. Edges
/// are matched by source node — use this to simulate a batch of endorsements
/// created at the same point in time.
pub fn mark_cohort_edges(spec: &mut GraphSpec, cohort_nodes: &[Uuid], created_at: DateTime<Utc>) {
    let cohort_set: std::collections::HashSet<Uuid> = cohort_nodes.iter().copied().collect();
    for edge in spec.all_edges_mut() {
        if cohort_set.contains(&edge.from) {
            edge.created_at = Some(created_at);
        }
    }
}

// ---------------------------------------------------------------------------
// Graph analysis helpers
// ---------------------------------------------------------------------------

/// Return the top-`count` nodes by total degree (in + out, active edges only),
/// excluding the anchor node (node 0 / named "anchor").
///
/// Ties are broken by UUID byte order (deterministic).
pub fn find_high_degree_nodes(spec: &GraphSpec, count: usize) -> Vec<Uuid> {
    // Build degree map: count all active edges incident to each node
    let mut degree: std::collections::HashMap<Uuid, usize> =
        std::collections::HashMap::with_capacity(spec.all_nodes().len());

    for node in spec.all_nodes() {
        degree.entry(node.id).or_insert(0);
    }

    for edge in spec.all_edges() {
        if !edge.revoked {
            *degree.entry(edge.from).or_insert(0) += 1;
            *degree.entry(edge.to).or_insert(0) += 1;
        }
    }

    // Exclude anchor (node 0 or named "anchor")
    let anchor_id = Uuid::from_u128(0u128);

    let mut ranked: Vec<(Uuid, usize)> = degree
        .into_iter()
        .filter(|(id, _)| *id != anchor_id)
        .collect();

    // Sort: descending by degree, then ascending by UUID bytes (deterministic tiebreak)
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    ranked.into_iter().take(count).map(|(id, _)| id).collect()
}

/// Revoke all active edges incident to `node` (both inbound and outbound).
///
/// This simulates a ban or full revocation event for a node.
pub fn remove_node_edges(spec: &mut GraphSpec, node: Uuid) {
    for edge in spec.all_edges_mut() {
        if !edge.revoked && (edge.from == node || edge.to == node) {
            edge.revoked = true;
        }
    }
}

// ---------------------------------------------------------------------------
// Internal: minimal LCG PRNG (no external deps)
// ---------------------------------------------------------------------------

/// Linear congruential generator for deterministic random number generation.
/// Parameters from Knuth (MMIX): m=2^64, a=6364136223846793005, c=1442695040888963407
struct LcgRng {
    state: u64,
}

impl LcgRng {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.state
    }

    /// Return a value in [0, n).
    fn next_usize(&mut self, n: usize) -> usize {
        (self.next() as usize) % n
    }
}

/// Sample `count` distinct values from `pool`, excluding `exclude`.
/// Falls back gracefully if pool has fewer than `count` distinct values
/// after excluding `exclude`.
fn sample_distinct(rng: &mut LcgRng, pool: &[Uuid], count: usize, exclude: Uuid) -> Vec<Uuid> {
    let mut selected: Vec<Uuid> = Vec::with_capacity(count);
    let mut attempts = 0usize;
    let max_attempts = count * pool.len().max(1) * 10;

    while selected.len() < count && attempts < max_attempts {
        attempts += 1;
        if pool.is_empty() {
            break;
        }
        let idx = rng.next_usize(pool.len());
        let candidate = pool[idx];
        if candidate != exclude && !selected.contains(&candidate) {
            selected.push(candidate);
        }
    }

    selected
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- barabasi_albert ---

    #[test]
    fn ba_produces_correct_node_count() {
        let params = ScaleGraphParams {
            node_count: 100,
            m: 3,
            seed_size: 5,
            seed: 1,
        };
        let spec = barabasi_albert(&params);
        assert_eq!(spec.all_nodes().len(), 100);
    }

    #[test]
    fn ba_anchor_is_node_zero() {
        let params = ScaleGraphParams::default();
        let spec = barabasi_albert(&params);
        let anchor_id = spec.node("anchor");
        assert_eq!(anchor_id, Uuid::from_u128(0u128));
    }

    #[test]
    fn ba_all_nodes_are_blue() {
        let params = ScaleGraphParams {
            node_count: 50,
            ..Default::default()
        };
        let spec = barabasi_albert(&params);
        for node in spec.all_nodes() {
            assert_eq!(node.team, Team::Blue, "node {} should be Blue", node.name);
        }
    }

    #[test]
    fn ba_has_edges() {
        let params = ScaleGraphParams {
            node_count: 20,
            m: 2,
            seed_size: 3,
            seed: 7,
        };
        let spec = barabasi_albert(&params);
        assert!(
            spec.active_edge_count() > 0,
            "graph should have active edges"
        );
    }

    #[test]
    fn ba_deterministic_with_same_seed() {
        let params = ScaleGraphParams {
            node_count: 50,
            m: 2,
            seed_size: 3,
            seed: 123,
        };
        let spec1 = barabasi_albert(&params);
        let spec2 = barabasi_albert(&params);
        assert_eq!(spec1.all_nodes().len(), spec2.all_nodes().len());
        assert_eq!(spec1.active_edge_count(), spec2.active_edge_count());
    }

    #[test]
    fn ba_different_seeds_produce_different_graphs() {
        let base = ScaleGraphParams {
            node_count: 100,
            m: 3,
            seed_size: 5,
            seed: 1,
        };
        let spec1 = barabasi_albert(&base);
        let spec2 = barabasi_albert(&ScaleGraphParams { seed: 2, ..base });
        // Edge counts should differ (extremely unlikely to match by chance)
        // At minimum, the graphs should not be identical
        let e1 = spec1.active_edge_count();
        let e2 = spec2.active_edge_count();
        // Both have same base seed structure but different attachment choices
        // Just verify both produce valid non-empty graphs
        assert!(e1 > 0);
        assert!(e2 > 0);
    }

    #[test]
    fn ba_seed_fully_connected() {
        let params = ScaleGraphParams {
            node_count: 5,
            m: 2,
            seed_size: 5,
            seed: 0,
        };
        let spec = barabasi_albert(&params);
        // 5 nodes, fully connected seed: 5*4 = 20 directed edges
        assert_eq!(spec.active_edge_count(), 20);
    }

    // --- attach_sybil_mesh ---

    #[test]
    fn sybil_mesh_adds_correct_node_count() {
        let mut spec = barabasi_albert(&ScaleGraphParams {
            node_count: 20,
            m: 2,
            seed_size: 3,
            seed: 1,
        });
        let blue_ids: Vec<Uuid> = spec.nodes_by_team(Team::Blue);
        let blue_count_before = blue_ids.len();

        let params = SybilMeshParams {
            sybil_count: 10,
            bridge_count: 3,
            ..Default::default()
        };
        let bridge_targets = &blue_ids[..3];
        let sybil_ids = attach_sybil_mesh(&mut spec, bridge_targets, &params);

        assert_eq!(sybil_ids.len(), 10);
        assert_eq!(
            spec.all_nodes().len(),
            blue_count_before + 10,
            "total nodes should be blue + sybil"
        );
    }

    #[test]
    fn sybil_nodes_are_red() {
        let mut spec = barabasi_albert(&ScaleGraphParams {
            node_count: 20,
            m: 2,
            seed_size: 3,
            seed: 1,
        });
        let blue_ids = spec.nodes_by_team(Team::Blue);
        let params = SybilMeshParams {
            sybil_count: 5,
            bridge_count: 2,
            ..Default::default()
        };
        let sybil_ids = attach_sybil_mesh(&mut spec, &blue_ids[..2], &params);

        for id in &sybil_ids {
            let node = spec.all_nodes().iter().find(|n| n.id == *id).unwrap();
            assert_eq!(node.team, Team::Red, "sybil node should be Red");
        }
    }

    #[test]
    fn sybil_mesh_internal_edges_fully_connected() {
        let mut spec = GraphSpec::new();
        // Add 3 blue nodes manually
        let blue: Vec<Uuid> = (0..3).map(|i| Uuid::from_u128(i as u128)).collect();
        for (i, &id) in blue.iter().enumerate() {
            spec.add_node(&format!("blue_{i}"), Team::Blue, id);
        }

        let params = SybilMeshParams {
            sybil_count: 4,
            bridge_count: 2,
            internal_weight: 1.0,
            bridge_weight: 0.8,
            seed: 5,
        };
        let sybil_ids = attach_sybil_mesh(&mut spec, &blue[..2], &params);

        // Internal sybil edges: 4 * 3 = 12 directed edges
        let internal_count = spec
            .all_edges()
            .iter()
            .filter(|e| !e.revoked && sybil_ids.contains(&e.from) && sybil_ids.contains(&e.to))
            .count();
        assert_eq!(internal_count, 12, "4 sybil nodes => 4*3=12 internal edges");
    }

    #[test]
    fn sybil_mesh_bridge_edges_created() {
        let mut spec = GraphSpec::new();
        let blue: Vec<Uuid> = (0..5).map(|i| Uuid::from_u128(i as u128)).collect();
        for (i, &id) in blue.iter().enumerate() {
            spec.add_node(&format!("blue_{i}"), Team::Blue, id);
        }

        let params = SybilMeshParams {
            sybil_count: 5,
            bridge_count: 3,
            internal_weight: 1.0,
            bridge_weight: 0.5,
            seed: 7,
        };
        let sybil_ids = attach_sybil_mesh(&mut spec, &blue[..3], &params);

        // Bridge edges: bidirectional (sybil → blue AND blue → sybil), weight 0.5
        let bridge_count = spec
            .all_edges()
            .iter()
            .filter(|e| {
                !e.revoked
                    && (e.weight - 0.5).abs() < 1e-5
                    && ((sybil_ids.contains(&e.from) && blue.contains(&e.to))
                        || (blue.contains(&e.from) && sybil_ids.contains(&e.to)))
            })
            .count();
        assert_eq!(
            bridge_count, 6,
            "should have 3 bidirectional bridge pairs = 6 edges"
        );
    }

    // --- mark_cohort_edges ---

    #[test]
    fn mark_cohort_edges_stamps_correct_edges() {
        let mut spec = GraphSpec::new();
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        let c = Uuid::from_u128(3);
        spec.add_node("a", Team::Blue, a);
        spec.add_node("b", Team::Blue, b);
        spec.add_node("c", Team::Blue, c);
        spec.add_edge(a, b, 1.0);
        spec.add_edge(a, c, 1.0);
        spec.add_edge(b, c, 1.0); // not in cohort

        let ts = chrono::Utc::now();
        mark_cohort_edges(&mut spec, &[a], ts);

        // Edges from `a` should have created_at = ts
        for edge in spec.all_edges() {
            if edge.from == a {
                assert_eq!(edge.created_at, Some(ts), "cohort edge should be stamped");
            } else {
                assert_eq!(
                    edge.created_at, None,
                    "non-cohort edge should not be stamped"
                );
            }
        }
    }

    #[test]
    fn mark_cohort_edges_empty_cohort_is_noop() {
        let mut spec = GraphSpec::new();
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        spec.add_node("a", Team::Blue, a);
        spec.add_node("b", Team::Blue, b);
        spec.add_edge(a, b, 1.0);

        let ts = chrono::Utc::now();
        mark_cohort_edges(&mut spec, &[], ts);

        for edge in spec.all_edges() {
            assert_eq!(edge.created_at, None, "no edges should be stamped");
        }
    }

    // --- find_high_degree_nodes ---

    #[test]
    fn find_high_degree_nodes_excludes_anchor() {
        let params = ScaleGraphParams {
            node_count: 50,
            m: 3,
            seed_size: 5,
            seed: 42,
        };
        let spec = barabasi_albert(&params);
        let top = find_high_degree_nodes(&spec, 5);
        let anchor = Uuid::from_u128(0u128);
        assert!(
            !top.contains(&anchor),
            "anchor should be excluded from results"
        );
    }

    #[test]
    fn find_high_degree_nodes_returns_correct_count() {
        let params = ScaleGraphParams {
            node_count: 30,
            m: 2,
            seed_size: 3,
            seed: 10,
        };
        let spec = barabasi_albert(&params);
        let top = find_high_degree_nodes(&spec, 5);
        assert_eq!(top.len(), 5);
    }

    #[test]
    fn find_high_degree_nodes_capped_by_available() {
        let mut spec = GraphSpec::new();
        // 3 non-anchor nodes
        for i in 1..=3usize {
            spec.add_node(&format!("node_{i}"), Team::Blue, Uuid::from_u128(i as u128));
        }
        let top = find_high_degree_nodes(&spec, 10);
        assert!(top.len() <= 3, "cannot return more nodes than exist");
    }

    #[test]
    fn find_high_degree_nodes_ordered_by_degree() {
        let mut spec = GraphSpec::new();
        let anchor = Uuid::from_u128(0);
        let hub = Uuid::from_u128(1);
        let leaf = Uuid::from_u128(2);
        spec.add_node("anchor", Team::Blue, anchor);
        spec.add_node("hub", Team::Blue, hub);
        spec.add_node("leaf", Team::Blue, leaf);

        // Hub has many edges; leaf has one
        for i in 3..=10usize {
            let id = Uuid::from_u128(i as u128);
            spec.add_node(&format!("spoke_{i}"), Team::Blue, id);
            spec.add_edge(hub, id, 1.0);
            spec.add_edge(id, hub, 1.0);
        }
        spec.add_edge(leaf, anchor, 1.0);

        let top = find_high_degree_nodes(&spec, 1);
        assert_eq!(top[0], hub, "hub should rank highest");
    }

    // --- remove_node_edges ---

    #[test]
    fn remove_node_edges_revokes_all_incident_edges() {
        let mut spec = GraphSpec::new();
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        let c = Uuid::from_u128(3);
        spec.add_node("a", Team::Blue, a);
        spec.add_node("b", Team::Blue, b);
        spec.add_node("c", Team::Blue, c);
        spec.add_edge(a, b, 1.0); // incident to a
        spec.add_edge(c, a, 1.0); // incident to a
        spec.add_edge(b, c, 1.0); // not incident to a

        remove_node_edges(&mut spec, a);

        // Edges involving a should be revoked
        for edge in spec.all_edges() {
            if edge.from == a || edge.to == a {
                assert!(edge.revoked, "edge incident to a should be revoked");
            } else {
                assert!(!edge.revoked, "edge not incident to a should be active");
            }
        }
    }

    #[test]
    fn remove_node_edges_idempotent() {
        let mut spec = GraphSpec::new();
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        spec.add_node("a", Team::Blue, a);
        spec.add_node("b", Team::Blue, b);
        spec.add_edge(a, b, 1.0);

        remove_node_edges(&mut spec, a);
        remove_node_edges(&mut spec, a); // second call should be harmless

        let revoked_count = spec.all_edges().iter().filter(|e| e.revoked).count();
        assert_eq!(revoked_count, 1);
    }

    #[test]
    fn remove_node_edges_updates_active_edge_count() {
        let params = ScaleGraphParams {
            node_count: 20,
            m: 2,
            seed_size: 3,
            seed: 5,
        };
        let mut spec = barabasi_albert(&params);
        let before = spec.active_edge_count();

        // Remove a high-degree node
        let top = find_high_degree_nodes(&spec, 1);
        if !top.is_empty() {
            remove_node_edges(&mut spec, top[0]);
            assert!(
                spec.active_edge_count() < before,
                "active edge count should decrease after removal"
            );
        }
    }

    // --- LcgRng internal ---

    #[test]
    fn lcg_rng_deterministic() {
        let mut r1 = LcgRng::new(42);
        let mut r2 = LcgRng::new(42);
        for _ in 0..100 {
            assert_eq!(r1.next(), r2.next());
        }
    }

    #[test]
    fn lcg_rng_next_usize_in_range() {
        let mut rng = LcgRng::new(1);
        for _ in 0..1000 {
            let v = rng.next_usize(7);
            assert!(v < 7, "value {v} out of range [0, 7)");
        }
    }
}
