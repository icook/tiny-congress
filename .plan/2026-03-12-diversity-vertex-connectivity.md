# Fix Diversity Approximation with Vertex Connectivity

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Issue:** #648
**Goal:** Replace the exploitable `COUNT(DISTINCT endorser_id)` diversity approximation with exact vertex connectivity via Edmonds-Karp max-flow.

**Architecture:** Load the endorsement subgraph into memory, split each intermediate node into in/out halves with capacity-1 edges, run BFS-based max-flow from anchor to each target. This gives the exact number of node-disjoint paths (Menger's theorem) — the correct Sybil resistance metric.

**Tech Stack:** Rust, sqlx (edge loading), Edmonds-Karp max-flow (pure algorithm, no new deps)

---

### Task 1: Create `max_flow.rs` — vertex connectivity algorithm

**Files:**
- Create: `service/src/trust/max_flow.rs`
- Modify: `service/src/trust/mod.rs` (add `pub mod max_flow;`)

**Context:**
The vertex connectivity between two nodes s and t in a directed graph equals the maximum number of internally vertex-disjoint paths from s to t. By Menger's theorem, this also equals the minimum vertex cut. We compute it by:
1. Splitting each intermediate node v into v_in and v_out with a capacity-1 edge
2. Source and target get infinite internal capacity (we don't count removing endpoints)
3. Original edges (u, v) become u_out → v_in with infinite capacity
4. Max-flow from source_out to target_in gives vertex connectivity

At demo scale (~100 nodes), a dense capacity matrix and BFS-based Edmonds-Karp is fast enough.

**Step 1: Write unit tests for the algorithm**

These are pure algorithm tests — no database, no async. Add them as `#[cfg(test)] mod tests` at the bottom of `max_flow.rs`.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_independent_paths() {
        // s → a → t, s → b → t
        let mut g = FlowGraph::new(4);
        let s = 0; let a = 1; let b = 2; let t = 3;
        g.add_edge(s, a);
        g.add_edge(a, t);
        g.add_edge(s, b);
        g.add_edge(b, t);
        assert_eq!(g.vertex_connectivity(s, t), 2);
    }

    #[test]
    fn shared_chokepoint() {
        // s → a → b → t, s → a → c → t (all paths through a)
        let mut g = FlowGraph::new(5);
        let s = 0; let a = 1; let b = 2; let c = 3; let t = 4;
        g.add_edge(s, a);
        g.add_edge(a, b);
        g.add_edge(a, c);
        g.add_edge(b, t);
        g.add_edge(c, t);
        assert_eq!(g.vertex_connectivity(s, t), 1);
    }

    #[test]
    fn hub_and_spoke() {
        // s → hub → spoke (only one path)
        let mut g = FlowGraph::new(3);
        g.add_edge(0, 1);
        g.add_edge(1, 2);
        assert_eq!(g.vertex_connectivity(0, 2), 1);
    }

    #[test]
    fn colluding_ring_single_attachment() {
        // s → bridge → r0 → r1 → r2 → r3 → r0 (ring)
        // All ring paths go through bridge
        let mut g = FlowGraph::new(6);
        let s = 0; let bridge = 1;
        let r0 = 2; let r1 = 3; let r2 = 4; let r3 = 5;
        g.add_edge(s, bridge);
        g.add_edge(bridge, r0);
        g.add_edge(r0, r1);
        g.add_edge(r1, r2);
        g.add_edge(r2, r3);
        g.add_edge(r3, r0);
        // Ring members endorsing each other
        g.add_edge(r1, r0);
        g.add_edge(r2, r1);
        g.add_edge(r3, r2);
        g.add_edge(r0, r3);
        // Vertex connectivity from s to any ring node = 1 (cut at bridge)
        assert_eq!(g.vertex_connectivity(s, r0), 1);
        assert_eq!(g.vertex_connectivity(s, r2), 1);
    }

    #[test]
    fn direct_edge() {
        // s → t directly
        let mut g = FlowGraph::new(2);
        g.add_edge(0, 1);
        assert_eq!(g.vertex_connectivity(0, 1), 1);
    }

    #[test]
    fn unreachable() {
        let mut g = FlowGraph::new(3);
        g.add_edge(0, 1);
        // 2 is not reachable from 0
        assert_eq!(g.vertex_connectivity(0, 2), 0);
    }

    #[test]
    fn three_independent_paths() {
        // s → a → t, s → b → t, s → c → t
        let mut g = FlowGraph::new(5);
        let s = 0; let a = 1; let b = 2; let c = 3; let t = 4;
        g.add_edge(s, a); g.add_edge(a, t);
        g.add_edge(s, b); g.add_edge(b, t);
        g.add_edge(s, c); g.add_edge(c, t);
        assert_eq!(g.vertex_connectivity(s, t), 3);
    }
}
```

**Step 2: Write the FlowGraph implementation**

```rust
//! Vertex connectivity via Edmonds-Karp max-flow on a vertex-split graph.

use std::collections::VecDeque;

/// Directed graph supporting vertex connectivity queries via max-flow.
///
/// Nodes are identified by `usize` indices in `[0, n)`. The caller is
/// responsible for mapping domain IDs (e.g. `Uuid`) to indices.
pub struct FlowGraph {
    n: usize,
    adj: Vec<Vec<usize>>,
}

impl FlowGraph {
    pub fn new(n: usize) -> Self {
        Self {
            n,
            adj: vec![Vec::new(); n],
        }
    }

    pub fn add_edge(&mut self, from: usize, to: usize) {
        self.adj[from].push(to);
    }

    /// Compute vertex connectivity from `source` to `target`.
    ///
    /// Returns the maximum number of internally vertex-disjoint paths,
    /// which equals the minimum vertex cut (Menger's theorem).
    pub fn vertex_connectivity(&self, source: usize, target: usize) -> i32 {
        if source == target || source >= self.n || target >= self.n {
            return 0;
        }

        // Build vertex-split flow network:
        // Node i splits into i_in (2*i) and i_out (2*i+1).
        // Internal edge i_in → i_out has capacity 1 (intermediate nodes)
        // or infinity (source/target — we don't cut endpoints).
        // Original edge (u, v) becomes u_out → v_in with infinite capacity.
        let nn = 2 * self.n;
        let inf = self.n as i32 + 1; // capacity larger than any possible flow
        let mut cap = vec![vec![0i32; nn]; nn];

        for i in 0..self.n {
            cap[2 * i][2 * i + 1] = if i == source || i == target { inf } else { 1 };
        }
        for u in 0..self.n {
            for &v in &self.adj[u] {
                cap[2 * u + 1][2 * v] = inf;
            }
        }

        let s = 2 * source + 1; // source_out
        let t = 2 * target;     // target_in
        edmonds_karp(&mut cap, nn, s, t)
    }
}

/// BFS-based max-flow (Edmonds-Karp).
fn edmonds_karp(cap: &mut [Vec<i32>], n: usize, source: usize, sink: usize) -> i32 {
    let mut total_flow = 0;
    loop {
        // BFS to find shortest augmenting path
        let mut parent = vec![usize::MAX; n];
        parent[source] = source;
        let mut queue = VecDeque::new();
        queue.push_back(source);

        while let Some(u) = queue.pop_front() {
            if u == sink { break; }
            for v in 0..n {
                if parent[v] == usize::MAX && cap[u][v] > 0 {
                    parent[v] = u;
                    queue.push_back(v);
                }
            }
        }

        if parent[sink] == usize::MAX { break; } // no augmenting path

        // Find bottleneck
        let mut flow = i32::MAX;
        let mut v = sink;
        while v != source {
            let u = parent[v];
            flow = flow.min(cap[u][v]);
            v = u;
        }

        // Update residual capacities
        let mut v = sink;
        while v != source {
            let u = parent[v];
            cap[u][v] -= flow;
            cap[v][u] += flow;
            v = u;
        }

        total_flow += flow;
    }
    total_flow
}
```

**Step 3: Add module to mod.rs**

Add `pub mod max_flow;` to `service/src/trust/mod.rs`.

**Step 4: Run unit tests**

```bash
cargo test -p tinycongress-api max_flow -- --nocapture
```

Expected: all 7 tests pass.

**Step 5: Commit**

```bash
git add service/src/trust/max_flow.rs service/src/trust/mod.rs
git commit -m "feat(trust): add vertex connectivity via Edmonds-Karp max-flow"
```

---

### Task 2: Replace diversity computation in engine.rs + add topic filter

**Files:**
- Modify: `service/src/trust/engine.rs`

**Context:**
Replace the SQL-based `COUNT(DISTINCT endorser_id)` with:
1. Load endorsement edges between reachable nodes
2. Build a `FlowGraph` mapping Uuid → index
3. For each reachable node, compute `vertex_connectivity(anchor, node)`

Also add `AND e.topic = 'trust'` to both the distance CTE and the new edge-loading query.

**Step 1: Update `compute_diversity_from`**

Replace the body of `compute_diversity_from` with:
1. Get reachable set via `compute_distances_from`
2. Load edges from DB (new query with topic filter)
3. Build `FlowGraph` with Uuid → index mapping
4. Compute connectivity for each non-anchor reachable node

New SQL for edge loading:
```sql
SELECT endorser_id, subject_id
FROM reputation__endorsements
WHERE revoked_at IS NULL
  AND endorser_id IS NOT NULL
  AND topic = 'trust'
  AND endorser_id = ANY($1)
  AND subject_id = ANY($1)
```

**Step 2: Add topic filter to distance CTE**

Add `AND e.topic = 'trust'` to both the base case and recursive case of the CTE in `compute_distances_from`.

**Step 3: Clean up unused types**

Remove `DiversityRow` struct (no longer needed — edge loading returns `(Uuid, Uuid)` pairs, not diversity rows).

**Step 4: Update doc comments**

Update the doc comment on `compute_diversity_from` to describe vertex connectivity instead of the approximation. Update `ComputedScore::path_diversity` doc comment.

**Step 5: Run backend tests**

```bash
cargo test -p tinycongress-api -- --nocapture
```

Expected: compilation succeeds. Some diversity tests may fail (expected — assertions update in Task 3).

**Step 6: Commit**

```bash
git add service/src/trust/engine.rs
git commit -m "feat(trust): replace diversity approximation with vertex connectivity

The old COUNT(DISTINCT endorser_id) approximation was exploitable by dense
adversarial clusters (simulation #643 proved colluding rings got diversity=2,
fully-connected red clusters got diversity=4-5 despite single attachment point).

The new implementation loads the reachable subgraph into memory and runs
Edmonds-Karp max-flow on a vertex-split graph to compute exact node-disjoint
path counts (Menger's theorem).

Also adds missing topic='trust' filter to both distance CTE and edge loading.

Fixes #648"
```

---

### Task 3: Update test assertions

**Files:**
- Modify: `service/tests/trust_engine_tests.rs`

**Step 1: Fix `test_path_diversity_shared_branch`**

Change assertion from:
```rust
assert!(*x_diversity >= 1, ...);
```
To:
```rust
assert_eq!(*x_diversity, 1, "Both paths to X share chokepoint A — vertex connectivity is 1");
```

Update the test's doc comment to explain that vertex connectivity correctly identifies the shared chokepoint.

**Step 2: Verify `test_path_diversity_independent_branches` still asserts `== 2`**

The topology `Seed → A → X` + `Seed → B → X` has two genuinely independent paths (A and B are independently reachable from Seed). Vertex connectivity should be 2. No change needed.

**Step 3: Verify `test_hub_and_spoke_diversity` still asserts `== 1`**

No change needed — hub-and-spoke was already correct.

**Step 4: Run all trust engine tests**

```bash
cargo test --test trust_engine_tests -- --nocapture
```

Expected: all tests pass.

**Step 5: Commit**

```bash
git add service/tests/trust_engine_tests.rs
git commit -m "test(trust): update diversity assertions for vertex connectivity

The shared-branch test now correctly asserts diversity=1 (both paths share
chokepoint A) instead of the previous >= 1 workaround for the approximation."
```

---

### Task 4: Run full test suite and verify

**Step 1: Run lint**

```bash
just lint-backend
```

**Step 2: Run all backend tests**

```bash
just test-backend
```

**Step 3: Verify no regressions**

All trust_engine_tests, trust_service_tests, trust_http_tests, trust_e2e_tests should pass.

---

## Files touched

| File | Action | Purpose |
|------|--------|---------|
| `service/src/trust/max_flow.rs` | Create | Edmonds-Karp vertex connectivity algorithm |
| `service/src/trust/mod.rs` | Modify | Add `pub mod max_flow` |
| `service/src/trust/engine.rs` | Modify | Replace diversity computation, add topic filter |
| `service/tests/trust_engine_tests.rs` | Modify | Fix shared-branch assertion |

## What this enables

- Simulation tests on PR #643 will show correct diversity values after rebase
- Colluding ring: diversity=1 (was 2)
- Red cluster single attachment: diversity=1 (was 4-5)
- Foundation for trust threshold enforcement using reliable diversity metric
