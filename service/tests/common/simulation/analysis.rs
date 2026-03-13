//! Pure-Rust graph analysis: Dijkstra distances + Edmonds-Karp vertex connectivity.
//!
//! No database dependency. Operates directly on [`GraphSpec`] for scale testing.
//!
//! ## Algorithms
//!
//! - **Distances:** sparse Dijkstra with edge cost = 1.0/weight. CUTOFF = 10.0.
//! - **Diversity:** vertex-disjoint path count via Edmonds-Karp max-flow using
//!   `HashMap<(usize, usize), i32>` for residual capacities (O(E) space).

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};
use std::fmt;

use uuid::Uuid;

use super::GraphSpec;

/// Distance cutoff: nodes with cost > CUTOFF are considered unreachable.
const CUTOFF: f32 = 10.0;

// ---------------------------------------------------------------------------
// ScaleAnalysis
// ---------------------------------------------------------------------------

/// Full graph analysis results relative to an anchor node.
pub struct ScaleAnalysis {
    pub anchor: Uuid,
    /// Trust-distance (Dijkstra cost) from anchor to each reachable node.
    /// Does NOT contain the anchor itself or unreachable nodes.
    pub distances: HashMap<Uuid, f32>,
    /// Vertex-disjoint path count from anchor to each sampled node.
    pub diversities: HashMap<Uuid, i32>,
}

impl ScaleAnalysis {
    /// Fraction of nodes eligible: distance ≤ max_distance AND diversity ≥ min_diversity.
    ///
    /// Returns 0.0 if no diversities have been computed.
    pub fn eligible_fraction(&self, max_distance: f32, min_diversity: i32) -> f64 {
        if self.diversities.is_empty() {
            return 0.0;
        }
        let eligible = self
            .diversities
            .iter()
            .filter(|(id, &div)| {
                let dist = self.distances.get(*id).copied().unwrap_or(f32::INFINITY);
                dist <= max_distance && div >= min_diversity
            })
            .count();
        eligible as f64 / self.diversities.len() as f64
    }

    /// Fraction of non-anchor nodes reachable from the anchor.
    ///
    /// Takes `total_nodes` (the full graph node count including anchor) as a
    /// parameter because `distances` only contains reachable nodes — comparing
    /// `distances.len()` to itself would always return 1.0.
    pub fn reachable_fraction(&self, total_nodes: usize) -> f64 {
        // distances.len() = reachable count (excludes anchor itself)
        // total_nodes - 1 = all non-anchor nodes
        self.distances.len() as f64 / (total_nodes.saturating_sub(1)).max(1) as f64
    }

    /// Statistics for the distance distribution across all reachable nodes.
    pub fn distance_stats(&self) -> DistributionStats {
        let mut values: Vec<f64> = self.distances.values().map(|&d| d as f64).collect();
        DistributionStats::from_values(&mut values)
    }

    /// Statistics for the diversity distribution across all computed nodes.
    pub fn diversity_stats(&self) -> DistributionStats {
        let mut values: Vec<f64> = self.diversities.values().map(|&d| d as f64).collect();
        DistributionStats::from_values(&mut values)
    }
}

// ---------------------------------------------------------------------------
// DistributionStats
// ---------------------------------------------------------------------------

/// Summary statistics for a distribution of f64 values.
#[derive(Debug, Clone)]
pub struct DistributionStats {
    pub count: usize,
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub median: f64,
    pub p90: f64,
    pub p99: f64,
}

impl DistributionStats {
    /// Compute stats from a mutable slice (sorts in place for percentiles).
    pub fn from_values(values: &mut Vec<f64>) -> Self {
        if values.is_empty() {
            return Self {
                count: 0,
                min: 0.0,
                max: 0.0,
                mean: 0.0,
                median: 0.0,
                p90: 0.0,
                p99: 0.0,
            };
        }
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
        let n = values.len();
        let min = values[0];
        let max = values[n - 1];
        let mean = values.iter().sum::<f64>() / n as f64;
        let median = percentile(values, 50.0);
        let p90 = percentile(values, 90.0);
        let p99 = percentile(values, 99.0);
        Self {
            count: n,
            min,
            max,
            mean,
            median,
            p90,
            p99,
        }
    }
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = (p / 100.0 * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

impl fmt::Display for DistributionStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "n={} min={:.2} max={:.2} mean={:.3} median={:.2} p90={:.2} p99={:.2}",
            self.count, self.min, self.max, self.mean, self.median, self.p90, self.p99
        )
    }
}

// ---------------------------------------------------------------------------
// Dijkstra
// ---------------------------------------------------------------------------

/// Ordered entry for Dijkstra's priority queue (min-heap by cost).
#[derive(Clone)]
struct DijkstraEntry {
    cost: f32,
    node: Uuid,
}

impl PartialEq for DijkstraEntry {
    fn eq(&self, other: &Self) -> bool {
        self.cost == other.cost && self.node == other.node
    }
}
impl Eq for DijkstraEntry {}

impl PartialOrd for DijkstraEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DijkstraEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reversed for min-heap
        other
            .cost
            .partial_cmp(&self.cost)
            .unwrap_or(Ordering::Equal)
    }
}

/// Compute trust distances from `anchor` to all reachable nodes via sparse Dijkstra.
///
/// Edge cost = 1.0 / weight. Nodes with accumulated cost > CUTOFF (10.0) are
/// not included in the result. The anchor itself is not present in the output.
pub fn compute_distances(spec: &GraphSpec, anchor: Uuid) -> HashMap<Uuid, f32> {
    // Build sparse adjacency list: from → [(to, cost)]
    let mut adj: HashMap<Uuid, Vec<(Uuid, f32)>> = HashMap::new();
    for edge in spec.all_edges() {
        if edge.revoked || edge.weight <= 0.0 {
            continue;
        }
        let cost = 1.0_f32 / edge.weight;
        adj.entry(edge.from).or_default().push((edge.to, cost));
    }

    let mut dist: HashMap<Uuid, f32> = HashMap::new();
    let mut heap = BinaryHeap::new();

    // Anchor starts at cost 0 but is not inserted into the result map
    heap.push(DijkstraEntry {
        cost: 0.0,
        node: anchor,
    });
    dist.insert(anchor, 0.0);

    while let Some(DijkstraEntry { cost, node }) = heap.pop() {
        // Skip if we already found a shorter path
        if let Some(&best) = dist.get(&node) {
            if cost > best {
                continue;
            }
        }

        if let Some(neighbors) = adj.get(&node) {
            for &(neighbor, edge_cost) in neighbors {
                let new_cost = cost + edge_cost;
                if new_cost > CUTOFF {
                    continue;
                }
                let entry = dist.entry(neighbor).or_insert(f32::INFINITY);
                if new_cost < *entry {
                    *entry = new_cost;
                    heap.push(DijkstraEntry {
                        cost: new_cost,
                        node: neighbor,
                    });
                }
            }
        }
    }

    // Remove anchor from result — callers only want other nodes
    dist.remove(&anchor);
    dist
}

// ---------------------------------------------------------------------------
// Edmonds-Karp vertex connectivity (sparse)
// ---------------------------------------------------------------------------

/// Compute vertex-disjoint path counts from `anchor` to each node in `targets`.
///
/// Uses sparse Edmonds-Karp max-flow with vertex splitting. Each non-anchor,
/// non-target node `v` is split into `v_in` and `v_out` with capacity 1, ensuring
/// paths are vertex-disjoint (not just edge-disjoint).
///
/// Returns a map of target → vertex connectivity (number of vertex-disjoint paths).
pub fn compute_diversity(spec: &GraphSpec, anchor: Uuid, targets: &[Uuid]) -> HashMap<Uuid, i32> {
    let mut result = HashMap::new();
    for &target in targets {
        if target == anchor {
            continue;
        }
        let flow = sparse_vertex_connectivity(spec, anchor, target);
        result.insert(target, flow);
    }
    result
}

/// Inner Edmonds-Karp max-flow for vertex-disjoint paths from `source` to `sink`.
///
/// Vertex splitting: each node `v` (except source/sink) becomes `v_in` → `v_out`
/// with capacity 1. Edges in the original graph become `u_out` → `v_in` with
/// capacity 1 (edges are the bottleneck for flow, but internal node capacity = 1
/// ensures vertex disjointness).
///
/// Node encoding: for a node with index `i`, `v_in = 2*i`, `v_out = 2*i + 1`.
///
/// BFS uses a sparse adjacency list built from the residual map to avoid O(n²)
/// neighbor iteration. On each BFS step, only edges that were ever inserted into
/// the residual map (i.e., real graph structure) are considered.
pub fn sparse_vertex_connectivity(spec: &GraphSpec, source: Uuid, sink: Uuid) -> i32 {
    let nodes = spec.all_nodes();
    let n = nodes.len();

    // Map Uuid → index
    let node_idx: HashMap<Uuid, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, node)| (node.id, i))
        .collect();

    let src_idx = match node_idx.get(&source) {
        Some(&i) => i,
        None => return 0,
    };
    let snk_idx = match node_idx.get(&sink) {
        Some(&i) => i,
        None => return 0,
    };

    // Node splitting: v_in = 2*i, v_out = 2*i + 1
    let src_out = 2 * src_idx + 1; // source emits from its _out side
    let snk_in = 2 * snk_idx; // sink receives at its _in side

    // Residual capacity map: (from, to) → remaining capacity
    let mut residual: HashMap<(usize, usize), i32> = HashMap::new();
    // Sparse adjacency: flow_node → [reachable flow_nodes] (includes reverse edges)
    let mut adj: HashMap<usize, Vec<usize>> = HashMap::new();

    // Helper: add a directed edge with capacity, plus its reverse edge (cap 0)
    macro_rules! add_edge {
        ($u:expr, $v:expr, $cap:expr) => {{
            let u = $u;
            let v = $v;
            let cap = $cap;
            *residual.entry((u, v)).or_insert(0) += cap;
            residual.entry((v, u)).or_insert(0);
            adj.entry(u).or_default().push(v);
            adj.entry(v).or_default().push(u);
        }};
    }

    // Internal edges: v_in → v_out, capacity 1 (enforces vertex disjointness)
    // Source and sink get large capacity so they don't become the bottleneck
    for i in 0..n {
        let cap = if i == src_idx || i == snk_idx {
            n as i32 // effectively unbounded within graph size
        } else {
            1
        };
        let v_in = 2 * i;
        let v_out = 2 * i + 1;
        add_edge!(v_in, v_out, cap);
    }

    // Graph edges: u_out → v_in, capacity 1
    for edge in spec.all_edges() {
        if edge.revoked {
            continue;
        }
        let u_idx = match node_idx.get(&edge.from) {
            Some(&i) => i,
            None => continue,
        };
        let v_idx = match node_idx.get(&edge.to) {
            Some(&i) => i,
            None => continue,
        };
        let u_out = 2 * u_idx + 1;
        let v_in = 2 * v_idx;
        add_edge!(u_out, v_in, 1);
    }

    // Edmonds-Karp: BFS augmenting paths using sparse adjacency
    let mut max_flow = 0;

    loop {
        // BFS from src_out to snk_in
        let mut parent: HashMap<usize, usize> = HashMap::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        queue.push_back(src_out);
        visited.insert(src_out);

        'bfs: while let Some(u) = queue.pop_front() {
            if let Some(neighbors) = adj.get(&u) {
                for &v in neighbors {
                    if !visited.contains(&v) {
                        let cap = residual.get(&(u, v)).copied().unwrap_or(0);
                        if cap > 0 {
                            parent.insert(v, u);
                            visited.insert(v);
                            if v == snk_in {
                                break 'bfs;
                            }
                            queue.push_back(v);
                        }
                    }
                }
            }
        }

        if !visited.contains(&snk_in) {
            break; // No augmenting path found
        }

        // Find bottleneck along path
        let mut flow = i32::MAX;
        let mut v = snk_in;
        while v != src_out {
            let u = parent[&v];
            let cap = residual[&(u, v)];
            flow = flow.min(cap);
            v = u;
        }

        // Update residual along path
        let mut v = snk_in;
        while v != src_out {
            let u = parent[&v];
            *residual.entry((u, v)).or_insert(0) -= flow;
            *residual.entry((v, u)).or_insert(0) += flow;
            v = u;
        }

        max_flow += flow;
    }

    max_flow
}

// ---------------------------------------------------------------------------
// High-level analysis functions
// ---------------------------------------------------------------------------

/// Full analysis: distances + diversity for all reachable nodes.
///
/// For large graphs this can be slow due to O(n) calls to Edmonds-Karp.
/// Use `analyze_graph_sampled` for graphs with > a few hundred nodes.
pub fn analyze_graph(spec: &GraphSpec, anchor: Uuid) -> ScaleAnalysis {
    let distances = compute_distances(spec, anchor);
    let targets: Vec<Uuid> = distances.keys().copied().collect();
    let diversities = compute_diversity(spec, anchor, &targets);
    ScaleAnalysis {
        anchor,
        distances,
        diversities,
    }
}

/// Sampled analysis: distances for all nodes, diversity for first N reachable nodes.
///
/// The sample is deterministic: it takes the first `diversity_sample` nodes from
/// the sorted key order of the distances map.
pub fn analyze_graph_sampled(
    spec: &GraphSpec,
    anchor: Uuid,
    diversity_sample: usize,
) -> ScaleAnalysis {
    let distances = compute_distances(spec, anchor);

    // Deterministic sample: collect keys, sort, take first N
    let mut sorted_keys: Vec<Uuid> = distances.keys().copied().collect();
    sorted_keys.sort(); // Uuid implements Ord — sort gives deterministic order
    let sample_targets: Vec<Uuid> = sorted_keys.into_iter().take(diversity_sample).collect();

    let diversities = compute_diversity(spec, anchor, &sample_targets);
    ScaleAnalysis {
        anchor,
        distances,
        diversities,
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::common::simulation::{GraphSpec, Team};

    fn make_node() -> Uuid {
        Uuid::new_v4()
    }

    /// Build a simple linear chain: anchor → a → b
    fn linear_chain() -> (GraphSpec, Uuid, Uuid, Uuid) {
        let mut spec = GraphSpec::new();
        let anchor = make_node();
        let a = make_node();
        let b = make_node();
        spec.add_node("anchor", Team::Blue, anchor);
        spec.add_node("a", Team::Blue, a);
        spec.add_node("b", Team::Blue, b);
        spec.add_edge(anchor, a, 1.0);
        spec.add_edge(a, b, 1.0);
        (spec, anchor, a, b)
    }

    // --- DistributionStats ---

    #[test]
    fn distribution_stats_empty() {
        let stats = DistributionStats::from_values(&mut vec![]);
        assert_eq!(stats.count, 0);
        assert_eq!(stats.min, 0.0);
        assert_eq!(stats.max, 0.0);
    }

    #[test]
    fn distribution_stats_single() {
        let stats = DistributionStats::from_values(&mut vec![5.0]);
        assert_eq!(stats.count, 1);
        assert!((stats.min - 5.0).abs() < 1e-6);
        assert!((stats.max - 5.0).abs() < 1e-6);
        assert!((stats.mean - 5.0).abs() < 1e-6);
    }

    #[test]
    fn distribution_stats_known_values() {
        // [1,2,3,4,5] → median=3, p90=5, p99=5, mean=3
        let mut v = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let stats = DistributionStats::from_values(&mut v);
        assert_eq!(stats.count, 5);
        assert!((stats.mean - 3.0).abs() < 1e-6);
        assert!((stats.median - 3.0).abs() < 1e-6);
        assert!((stats.min - 1.0).abs() < 1e-6);
        assert!((stats.max - 5.0).abs() < 1e-6);
    }

    #[test]
    fn distribution_stats_display() {
        let mut v = vec![1.0, 2.0, 3.0];
        let stats = DistributionStats::from_values(&mut v);
        let s = stats.to_string();
        assert!(s.contains("n=3"), "display should include count: {s}");
        assert!(s.contains("mean="), "display should include mean: {s}");
    }

    // --- compute_distances ---

    #[test]
    fn distances_linear_chain() {
        let (spec, anchor, a, b) = linear_chain();
        let dist = compute_distances(&spec, anchor);

        // a is 1 hop (cost=1.0), b is 2 hops (cost=2.0)
        assert!(dist.contains_key(&a), "a should be reachable");
        assert!(dist.contains_key(&b), "b should be reachable");
        assert!(
            !dist.contains_key(&anchor),
            "anchor should not be in result"
        );

        let da = dist[&a];
        let db = dist[&b];
        assert!((da - 1.0).abs() < 1e-5, "a cost should be 1.0, got {da}");
        assert!((db - 2.0).abs() < 1e-5, "b cost should be 2.0, got {db}");
    }

    #[test]
    fn distances_revoked_edge_ignored() {
        let mut spec = GraphSpec::new();
        let anchor = make_node();
        let a = make_node();
        spec.add_node("anchor", Team::Blue, anchor);
        spec.add_node("a", Team::Blue, a);
        spec.add_edge_revoked(anchor, a, 1.0);

        let dist = compute_distances(&spec, anchor);
        assert!(
            dist.is_empty(),
            "revoked edge should not contribute to reachability"
        );
    }

    #[test]
    fn distances_higher_weight_means_lower_cost() {
        // Two paths to b: anchor→a(w=0.5)→b(w=1.0) and anchor→c(w=1.0)→b(w=1.0)
        let mut spec = GraphSpec::new();
        let anchor = make_node();
        let a = make_node();
        let b = make_node();
        let c = make_node();
        spec.add_node("anchor", Team::Blue, anchor);
        spec.add_node("a", Team::Blue, a);
        spec.add_node("b", Team::Blue, b);
        spec.add_node("c", Team::Blue, c);
        spec.add_edge(anchor, a, 0.5); // cost = 2.0
        spec.add_edge(a, b, 1.0); // cost = 1.0 → total via a = 3.0
        spec.add_edge(anchor, c, 1.0); // cost = 1.0
        spec.add_edge(c, b, 1.0); // cost = 1.0 → total via c = 2.0

        let dist = compute_distances(&spec, anchor);
        // Dijkstra should find the shorter path: via c (cost=2.0)
        let db = dist[&b];
        assert!(
            (db - 2.0).abs() < 1e-5,
            "b should be reached via c (cost=2.0), got {db}"
        );
    }

    #[test]
    fn distances_cutoff_respected() {
        // Chain: anchor → a (w=1.0) repeated 15 times. Beyond 10 hops, unreachable.
        let mut spec = GraphSpec::new();
        let anchor = make_node();
        spec.add_node("anchor", Team::Blue, anchor);
        let mut prev = anchor;
        let mut nodes = vec![];
        for i in 0..15 {
            let n = make_node();
            spec.add_node(&format!("n{i}"), Team::Blue, n);
            spec.add_edge(prev, n, 1.0);
            nodes.push(n);
            prev = n;
        }

        let dist = compute_distances(&spec, anchor);
        // Nodes at distance ≤ 10.0 (first 10) should be reachable; beyond should not
        for (i, node) in nodes.iter().enumerate() {
            let expected_cost = (i + 1) as f32;
            if expected_cost <= 10.0 {
                assert!(
                    dist.contains_key(node),
                    "node {i} at cost {expected_cost} should be reachable"
                );
            } else {
                assert!(
                    !dist.contains_key(node),
                    "node {i} at cost {expected_cost} should be cut off"
                );
            }
        }
    }

    #[test]
    fn distances_isolated_node_unreachable() {
        let mut spec = GraphSpec::new();
        let anchor = make_node();
        let isolated = make_node();
        spec.add_node("anchor", Team::Blue, anchor);
        spec.add_node("isolated", Team::Blue, isolated);
        // No edges

        let dist = compute_distances(&spec, anchor);
        assert!(
            !dist.contains_key(&isolated),
            "isolated node should not be reachable"
        );
    }

    // --- sparse_vertex_connectivity ---

    #[test]
    fn connectivity_single_path() {
        let (spec, anchor, _a, b) = linear_chain();
        // anchor → a → b: exactly 1 vertex-disjoint path to b
        let flow = sparse_vertex_connectivity(&spec, anchor, b);
        assert_eq!(flow, 1, "linear chain has exactly 1 vertex-disjoint path");
    }

    #[test]
    fn connectivity_two_independent_paths() {
        // anchor → a → target
        // anchor → b → target
        let mut spec = GraphSpec::new();
        let anchor = make_node();
        let a = make_node();
        let b = make_node();
        let target = make_node();
        spec.add_node("anchor", Team::Blue, anchor);
        spec.add_node("a", Team::Blue, a);
        spec.add_node("b", Team::Blue, b);
        spec.add_node("target", Team::Blue, target);
        spec.add_edge(anchor, a, 1.0);
        spec.add_edge(anchor, b, 1.0);
        spec.add_edge(a, target, 1.0);
        spec.add_edge(b, target, 1.0);

        let flow = sparse_vertex_connectivity(&spec, anchor, target);
        assert_eq!(flow, 2, "two independent paths → diversity=2");
    }

    #[test]
    fn connectivity_shared_intermediate_node() {
        // anchor → bridge → a
        // anchor → bridge → b (but bridge is shared, limits to 1 path for each)
        let mut spec = GraphSpec::new();
        let anchor = make_node();
        let bridge = make_node();
        let target = make_node();
        spec.add_node("anchor", Team::Blue, anchor);
        spec.add_node("bridge", Team::Blue, bridge);
        spec.add_node("target", Team::Blue, target);
        spec.add_edge(anchor, bridge, 1.0);
        spec.add_edge(bridge, target, 1.0);

        // anchor → bridge → target: bridge is the single choke point
        let flow = sparse_vertex_connectivity(&spec, anchor, target);
        assert_eq!(flow, 1, "shared bridge limits to 1 vertex-disjoint path");
    }

    #[test]
    fn connectivity_three_independent_paths() {
        let mut spec = GraphSpec::new();
        let anchor = make_node();
        let target = make_node();
        spec.add_node("anchor", Team::Blue, anchor);
        spec.add_node("target", Team::Blue, target);

        // Three independent bridges
        for i in 0..3 {
            let mid = make_node();
            spec.add_node(&format!("mid{i}"), Team::Blue, mid);
            spec.add_edge(anchor, mid, 1.0);
            spec.add_edge(mid, target, 1.0);
        }

        let flow = sparse_vertex_connectivity(&spec, anchor, target);
        assert_eq!(flow, 3, "three independent bridges → diversity=3");
    }

    #[test]
    fn connectivity_revoked_path_ignored() {
        let mut spec = GraphSpec::new();
        let anchor = make_node();
        let a = make_node();
        let b = make_node();
        let target = make_node();
        spec.add_node("anchor", Team::Blue, anchor);
        spec.add_node("a", Team::Blue, a);
        spec.add_node("b", Team::Blue, b);
        spec.add_node("target", Team::Blue, target);

        // Two paths, one revoked
        spec.add_edge(anchor, a, 1.0);
        spec.add_edge(a, target, 1.0);
        spec.add_edge_revoked(anchor, b, 1.0); // revoked
        spec.add_edge(b, target, 1.0);

        let flow = sparse_vertex_connectivity(&spec, anchor, target);
        assert_eq!(flow, 1, "revoked path should not count");
    }

    #[test]
    fn connectivity_no_path_returns_zero() {
        let mut spec = GraphSpec::new();
        let anchor = make_node();
        let isolated = make_node();
        spec.add_node("anchor", Team::Blue, anchor);
        spec.add_node("isolated", Team::Blue, isolated);

        let flow = sparse_vertex_connectivity(&spec, anchor, isolated);
        assert_eq!(flow, 0, "no path → 0 vertex-disjoint paths");
    }

    // --- compute_diversity ---

    #[test]
    fn compute_diversity_basic() {
        let (spec, anchor, a, b) = linear_chain();
        let targets = vec![a, b];
        let divs = compute_diversity(&spec, anchor, &targets);

        assert_eq!(divs[&a], 1, "a has 1 path from anchor");
        assert_eq!(divs[&b], 1, "b has 1 path from anchor");
    }

    #[test]
    fn compute_diversity_skips_anchor() {
        let (spec, anchor, _a, _b) = linear_chain();
        let targets = vec![anchor]; // Should be skipped
        let divs = compute_diversity(&spec, anchor, &targets);
        assert!(
            divs.is_empty(),
            "anchor should be excluded from diversity results"
        );
    }

    // --- analyze_graph ---

    #[test]
    fn analyze_graph_basic() {
        let (spec, anchor, a, b) = linear_chain();
        let analysis = analyze_graph(&spec, anchor);

        assert_eq!(analysis.anchor, anchor);
        assert!(analysis.distances.contains_key(&a));
        assert!(analysis.distances.contains_key(&b));
        assert!(analysis.diversities.contains_key(&a));
        assert!(analysis.diversities.contains_key(&b));
    }

    #[test]
    fn analyze_graph_sampled_limits_diversity() {
        let mut spec = GraphSpec::new();
        let anchor = make_node();
        spec.add_node("anchor", Team::Blue, anchor);

        let mut nodes = vec![];
        for i in 0..5 {
            let n = make_node();
            spec.add_node(&format!("n{i}"), Team::Blue, n);
            spec.add_edge(anchor, n, 1.0);
            nodes.push(n);
        }

        let analysis = analyze_graph_sampled(&spec, anchor, 2);
        assert_eq!(
            analysis.distances.len(),
            5,
            "all 5 nodes should have distances"
        );
        assert_eq!(
            analysis.diversities.len(),
            2,
            "diversity sample should be limited to 2"
        );
    }

    // --- ScaleAnalysis methods ---

    #[test]
    fn reachable_fraction_all_reachable() {
        let (spec, anchor, _a, _b) = linear_chain();
        let analysis = analyze_graph(&spec, anchor);
        // 3 total nodes; anchor excluded from distances, 2 reachable
        let frac = analysis.reachable_fraction(3);
        assert!(
            (frac - 1.0).abs() < 1e-6,
            "all non-anchor nodes reachable → 1.0, got {frac}"
        );
    }

    #[test]
    fn reachable_fraction_partial() {
        let mut spec = GraphSpec::new();
        let anchor = make_node();
        let a = make_node();
        let isolated = make_node();
        spec.add_node("anchor", Team::Blue, anchor);
        spec.add_node("a", Team::Blue, a);
        spec.add_node("isolated", Team::Blue, isolated);
        spec.add_edge(anchor, a, 1.0);
        // isolated has no edges

        let analysis = analyze_graph(&spec, anchor);
        // 3 total nodes; 1 reachable (a), 1 not (isolated)
        let frac = analysis.reachable_fraction(3);
        assert!(
            (frac - 0.5).abs() < 1e-6,
            "1/2 non-anchor nodes reachable → 0.5, got {frac}"
        );
    }

    #[test]
    fn reachable_fraction_fix_does_not_return_always_one() {
        // This test verifies the bug is fixed:
        // the OLD version would return distances.len() / distances.len() = 1.0
        // even when some nodes are unreachable.
        let mut spec = GraphSpec::new();
        let anchor = make_node();
        spec.add_node("anchor", Team::Blue, anchor);
        for i in 0..3 {
            let n = make_node();
            spec.add_node(&format!("reachable_{i}"), Team::Blue, n);
            spec.add_edge(anchor, n, 1.0);
        }
        for i in 0..7 {
            let isolated_n = make_node();
            spec.add_node(&format!("isolated_{i}"), Team::Blue, isolated_n);
            // No edges — isolated
        }

        let analysis = analyze_graph(&spec, anchor);
        // 11 total: 1 anchor + 3 reachable + 7 isolated
        // reachable_fraction should be 3/10 = 0.3, NOT 1.0
        let frac = analysis.reachable_fraction(11);
        assert!(
            (frac - 0.3).abs() < 1e-6,
            "bug fix: should be 3/10 = 0.3, got {frac}"
        );
    }

    #[test]
    fn eligible_fraction_basic() {
        let mut spec = GraphSpec::new();
        let anchor = make_node();
        let target = make_node();
        spec.add_node("anchor", Team::Blue, anchor);
        spec.add_node("target", Team::Blue, target);
        spec.add_edge(anchor, target, 1.0);

        let analysis = analyze_graph(&spec, anchor);
        // distance = 1.0 ≤ 5.0, diversity = 1 ≥ 1 → eligible
        let frac = analysis.eligible_fraction(5.0, 1);
        assert!((frac - 1.0).abs() < 1e-6, "all nodes eligible: {frac}");

        // diversity threshold too high → not eligible
        let frac2 = analysis.eligible_fraction(5.0, 2);
        assert!(
            (frac2 - 0.0).abs() < 1e-6,
            "diversity threshold 2 → 0 eligible: {frac2}"
        );
    }

    #[test]
    fn eligible_fraction_empty_diversities() {
        let analysis = ScaleAnalysis {
            anchor: make_node(),
            distances: HashMap::new(),
            diversities: HashMap::new(),
        };
        assert_eq!(analysis.eligible_fraction(5.0, 2), 0.0);
    }

    #[test]
    fn distance_stats_basic() {
        let (spec, anchor, _a, _b) = linear_chain();
        let analysis = analyze_graph(&spec, anchor);
        let stats = analysis.distance_stats();
        assert_eq!(stats.count, 2);
        assert!((stats.min - 1.0).abs() < 1e-5);
        assert!((stats.max - 2.0).abs() < 1e-5);
    }

    #[test]
    fn diversity_stats_basic() {
        let mut spec = GraphSpec::new();
        let anchor = make_node();
        let target = make_node();
        spec.add_node("anchor", Team::Blue, anchor);
        spec.add_node("target", Team::Blue, target);
        spec.add_edge(anchor, target, 1.0);

        let analysis = analyze_graph(&spec, anchor);
        let stats = analysis.diversity_stats();
        assert_eq!(stats.count, 1);
        assert!((stats.min - 1.0).abs() < 1e-5);
    }
}
