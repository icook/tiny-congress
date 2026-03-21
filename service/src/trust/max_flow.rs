/// Vertex connectivity via Edmonds-Karp max-flow on a vertex-split graph.
///
/// Each node i is split into `i_in` (2*i) and `i_out` (2*i+1). Internal edges
/// carry capacity 1 for intermediate nodes, or n+1 for source/target (effectively
/// infinite). Original edge (u, v) becomes `u_out → v_in` with capacity 1.
/// Max-flow from `source_out` to `target_in` equals the vertex connectivity.
pub struct FlowGraph {
    n: usize,
    /// Directed edges stored as (from, to) pairs.
    edges: Vec<(usize, usize)>,
}

impl FlowGraph {
    /// Create a flow graph for `n` nodes (indices `0..n`).
    #[must_use]
    pub const fn new(n: usize) -> Self {
        Self {
            n,
            edges: Vec::new(),
        }
    }

    const fn node_in(i: usize) -> usize {
        2 * i
    }

    const fn node_out(i: usize) -> usize {
        2 * i + 1
    }

    /// Add a directed edge from `from` to `to`.
    pub fn add_edge(&mut self, from: usize, to: usize) {
        self.edges.push((from, to));
    }

    /// Build a fresh capacity matrix for the given source and target.
    ///
    /// Internal edges: capacity 1 for intermediate nodes, n+1 for source/target.
    /// Cross edges: capacity 1 per edge (prevents reuse of the same logical edge).
    fn build_cap(&self, source: usize, target: usize) -> Vec<Vec<i32>> {
        let n = self.n;
        // At demo scale n <= 100; i32 is wide enough and the truncation is safe.
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let inf = (n as i32) + 1;
        let size = 2 * n;
        let mut cap = vec![vec![0i32; size]; size];

        // Internal node edges
        for i in 0..n {
            let c = if i == source || i == target { inf } else { 1 };
            cap[Self::node_in(i)][Self::node_out(i)] = c;
        }

        // Cross edges from the graph structure
        for &(from, to) in &self.edges {
            cap[Self::node_out(from)][Self::node_in(to)] += 1;
        }

        cap
    }

    /// BFS to find an augmenting path in the residual graph.
    /// Returns the parent array if a path to `sink` was found, otherwise `None`.
    fn bfs(cap: &[Vec<i32>], source: usize, sink: usize) -> Option<Vec<usize>> {
        let size = cap.len();
        let sentinel = usize::MAX;
        let mut parent = vec![sentinel; size];
        parent[source] = source;
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(source);

        while let Some(u) = queue.pop_front() {
            for v in 0..size {
                if parent[v] == sentinel && cap[u][v] > 0 {
                    parent[v] = u;
                    if v == sink {
                        return Some(parent);
                    }
                    queue.push_back(v);
                }
            }
        }
        None
    }

    /// Compute the maximum number of internally vertex-disjoint paths from
    /// `source` to `target` using Edmonds-Karp (BFS-based Ford-Fulkerson).
    ///
    /// Returns 0 if source == target, either index is out of bounds, or
    /// target is unreachable from source.
    #[must_use]
    pub fn vertex_connectivity(&self, source: usize, target: usize) -> i32 {
        if source == target || source >= self.n || target >= self.n {
            return 0;
        }

        let mut cap = self.build_cap(source, target);
        let s = Self::node_out(source);
        let t = Self::node_in(target);
        let mut flow = 0i32;

        while let Some(parent) = Self::bfs(&cap, s, t) {
            // Find bottleneck along the path
            let mut path_flow = i32::MAX;
            let mut v = t;
            while v != s {
                let u = parent[v];
                path_flow = path_flow.min(cap[u][v]);
                v = u;
            }

            // Update residual capacities
            let mut v = t;
            while v != s {
                let u = parent[v];
                cap[u][v] -= path_flow;
                cap[v][u] += path_flow;
                v = u;
            }

            flow += path_flow;
        }

        flow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_independent_paths() {
        // s→a→t and s→b→t — two disjoint paths
        let mut g = FlowGraph::new(4);
        // nodes: 0=s, 1=a, 2=b, 3=t
        g.add_edge(0, 1);
        g.add_edge(1, 3);
        g.add_edge(0, 2);
        g.add_edge(2, 3);
        assert_eq!(g.vertex_connectivity(0, 3), 2);
    }

    #[test]
    fn shared_chokepoint() {
        // s→a→b→t and s→a→c→t — all paths go through a (chokepoint)
        let mut g = FlowGraph::new(5);
        // nodes: 0=s, 1=a, 2=b, 3=c, 4=t
        g.add_edge(0, 1);
        g.add_edge(1, 2);
        g.add_edge(2, 4);
        g.add_edge(1, 3);
        g.add_edge(3, 4);
        assert_eq!(g.vertex_connectivity(0, 4), 1);
    }

    #[test]
    fn hub_and_spoke() {
        // s→hub→spoke — only one path
        let mut g = FlowGraph::new(3);
        // nodes: 0=s, 1=hub, 2=spoke(t)
        g.add_edge(0, 1);
        g.add_edge(1, 2);
        assert_eq!(g.vertex_connectivity(0, 2), 1);
    }

    #[test]
    fn colluding_ring_single_attachment() {
        // s→bridge→ring (4 ring nodes in a cycle with bidirectional edges)
        // connectivity from s to any ring node should be 1 (only bridge connects)
        // nodes: 0=s, 1=bridge, 2=r0, 3=r1, 4=r2, 5=r3
        let mut g = FlowGraph::new(6);
        g.add_edge(0, 1); // s → bridge
        g.add_edge(1, 2); // bridge → ring entry
                          // Ring cycle (bidirectional)
        g.add_edge(2, 3);
        g.add_edge(3, 4);
        g.add_edge(4, 5);
        g.add_edge(5, 2);
        g.add_edge(3, 2);
        g.add_edge(4, 3);
        g.add_edge(5, 4);
        g.add_edge(2, 5);
        // Connectivity from s to any ring node is 1 (bridge is the chokepoint)
        assert_eq!(g.vertex_connectivity(0, 3), 1);
        assert_eq!(g.vertex_connectivity(0, 4), 1);
        assert_eq!(g.vertex_connectivity(0, 5), 1);
    }

    #[test]
    fn direct_edge() {
        // s→t directly
        let mut g = FlowGraph::new(2);
        g.add_edge(0, 1);
        assert_eq!(g.vertex_connectivity(0, 1), 1);
    }

    #[test]
    fn unreachable() {
        // s→a, t disconnected
        let mut g = FlowGraph::new(3);
        // nodes: 0=s, 1=a, 2=t
        g.add_edge(0, 1);
        // no path to t
        assert_eq!(g.vertex_connectivity(0, 2), 0);
    }

    #[test]
    fn three_independent_paths() {
        // s→a→t, s→b→t, s→c→t — three disjoint paths
        let mut g = FlowGraph::new(5);
        // nodes: 0=s, 1=a, 2=b, 3=c, 4=t
        g.add_edge(0, 1);
        g.add_edge(1, 4);
        g.add_edge(0, 2);
        g.add_edge(2, 4);
        g.add_edge(0, 3);
        g.add_edge(3, 4);
        assert_eq!(g.vertex_connectivity(0, 4), 3);
    }

    #[test]
    fn source_equals_target_returns_zero() {
        // Connectivity from a node to itself is not meaningful — guard returns 0.
        let mut g = FlowGraph::new(3);
        g.add_edge(0, 1);
        g.add_edge(1, 2);
        assert_eq!(g.vertex_connectivity(0, 0), 0);
        assert_eq!(g.vertex_connectivity(2, 2), 0);
    }

    #[test]
    fn out_of_bounds_source_returns_zero() {
        // An index >= n is not a valid node; guard returns 0.
        let mut g = FlowGraph::new(3);
        g.add_edge(0, 1);
        assert_eq!(g.vertex_connectivity(5, 1), 0);
    }

    #[test]
    fn out_of_bounds_target_returns_zero() {
        // An index >= n is not a valid node; guard returns 0.
        let mut g = FlowGraph::new(3);
        g.add_edge(0, 1);
        assert_eq!(g.vertex_connectivity(0, 5), 0);
    }
}
