# Adversarial Review: Trust Engine + Simulation Framework

**Date:** 2026-03-13
**Perspective:** Outside graph theory / trust systems expert, assuming fundamental flaws.
**Scope:** `max_flow.rs`, `engine.rs`, simulation harness (GraphBuilder, topology generators, SimulationReport), all 6 named scenarios, room constraints.

---

## 1. Distance cutoff prunes vertex connectivity scope

**Severity: Medium — exploitable at scale**

`compute_diversity_from` calls `compute_distances_from` to get the reachable set, then loads edges only within that set (`AND endorser_id = ANY($1) AND subject_id = ANY($1)`). The distance CTE uses a `distance < 10.0` cutoff.

This means the reachable set fed to the max-flow computation may be **incomplete**. If a node is unreachable via the CTE (distance cutoff, cycle pruning), it's excluded from the flow graph — even if it would have provided an alternative vertex-disjoint path.

**Concrete attack:** An adversary creates a topology where the shortest path to a target goes through a chokepoint (distance 4), but there's a longer alternative path (distance 9.5) through independent nodes. The CTE finds both → diversity 2. If the adversary lengthens the alternative path to distance 10.1, it's pruned from the reachable set → diversity drops to 1. The attacker controls diversity by manipulating edge weights on paths they don't even need.

**Root cause:** Using distance-based reachability to define the scope of a connectivity computation conflates two independent graph properties. Distance measures "how far" (cost); connectivity measures "how many independent ways" (redundancy). Pruning by distance before computing connectivity introduces a false dependency.

**Demo-scale impact:** Unlikely to manifest — the 10.0 cutoff is generous for ~100 users. Worth documenting as an architectural assumption.

---

## 2. CTE has no polynomial complexity guarantee

The CTE uses `UNION ALL` with per-path cycle prevention (`NOT (e.subject_id = ANY(tg.path))`). It explores **all acyclic paths** and picks `MIN(distance)`. This is correct for finding minimum distance but is O(exponential) in the worst case.

A fully connected subgraph of N nodes at weight 1.0 (cost 1.0 per hop, so 10 hops before cutoff) would enumerate combinatorial paths. PostgreSQL's CTE executor doesn't have a "visited set" optimization — it faithfully generates all of them.

**Demo-scale impact:** Fine at ~100 users. At 1,000+ users with dense clusters, this CTE becomes a DoS vector. A malicious user could construct a dense endorsement subgraph specifically to make the CTE expensive.

---

## 3. Simulation doesn't test materialization + constraint pipeline

**Severity: High — test coverage gap**

The simulation tests call `TrustEngine` directly — `compute_distances_from` and `compute_diversity_from`. But the actual room eligibility check goes through:

1. `recompute_from_anchor` writes to `trust__score_snapshots`
2. `RoomConstraint::check` reads from `trust__score_snapshots` via `TrustRepo::get_score`

The simulation **never exercises the materialization step**. It never calls `recompute_from_anchor`, never checks `CommunityConstraint` or `CongressConstraint` against the scenario topologies.

A bug in `recompute_from_anchor` (e.g., writing `diversity = 0` for unreachable nodes, off-by-one in the anchor sentinel), or in the constraint check (e.g., comparing `>=` vs `>` for `min_diversity`), would be invisible to the simulation.

**The simulation validates the computation but not the decision.** In trust system terms: it tests "does the algorithm produce the right scores?" but not "do the right scores produce the right access decisions?" The gap between engine output and constraint input is exactly where bugs hide — serialization, nullability, default values, comparison operators.

---

## 4. Cross-edge capacity `+= 1` relies on uniqueness constraint

In `build_cap`:
```rust
cap[Self::node_out(from)][Self::node_in(to)] += 1;
```

If the same `(from, to)` endorsement exists twice in the database, the cross-edge gets capacity 2, allowing 2 units of flow through what should be a single edge. The code relies on a uniqueness constraint that isn't verified at the code level.

The `insert_endorsement` factory has no `ON CONFLICT` clause. If a test accidentally creates two endorsements from A→B, the `+=` accumulates and max-flow overestimates connectivity.

**Severity: Low** — only if schema invariant is violated.

---

## 5. Scenarios are not adversarial enough

All 6 scenarios test topologies where the adversary's behavior is structurally obvious. Single attachment point, textbook patterns. Missing:

### 5a. Multi-point attachment
Red cluster connected to the blue network at 2+ points. Current scenarios always use a single bridge (diversity = 1). What if the adversary compromises 2 blue nodes? Expected diversity = 2 — does the system correctly recognize this as "adequate diversity" even though the nodes are adversarial?

### 5b. Asymmetric weight exploitation
Attacker uses super-high-weight edges (e.g., 10.0) to stay near the anchor (cost 0.1) while having low structural diversity. None of the scenarios test this.

### 5c. Constraint-level decisions
No scenario runs `CommunityConstraint` or `CongressConstraint` against the topology. We don't know if the computed scores actually produce correct access control decisions.

### 5d. Graph-splitting attack
Adversary positions as a cut vertex for legitimate blue nodes. Denouncing them disconnects innocents. This tests blue resilience, not just red blocking — the dual of the diversity problem.

### 5e. Near-zero weight edges
Weight = 0.001, cost = 1000.0 — far beyond cutoff. An endorsement with effectively zero weight is functionally nonexistent. Is this validated? Can an attacker create "phantom edges" that exist in the DB but contribute nothing?

---

## 6. Stale comments reference old approximation

After PR #652, diversity is exact vertex connectivity, not an approximation. Several simulation test comments still say:

- `sim_colluding_ring`: "does the diversity approximation count ring members as distinct endorsers?"
- `sim_colluding_ring`: "This is a known limitation of the approximation at demo scale."
- `sim_red_cluster_single_attachment`: `"(approximation inflated)"` in eprintln

These should reference vertex connectivity.

---

## 7. `healthy_web` deterministic hash is weak

```rust
let hash = ((i * 7 + j * 13 + 37) % 100) as f64 / 100.0;
```

This isn't a hash — it's a linear function with strong periodicity. The generated topology may have structural properties (bipartite-like, regular degree) that don't appear in real social graphs. If the "blue team behaves correctly" assertion only holds for this specific structure, the simulation's validity claim is weaker than it appears.

---

## 8. Double distance computation in `recompute_from_anchor`

`recompute_from_anchor` calls both `compute_distances_from` and `compute_diversity_from`. But `compute_diversity_from` internally calls `compute_distances_from` again. The distance CTE runs twice. Harmless at demo scale, compounds with the CTE's exponential-case behavior (issue #2).

---

## Severity Summary

| # | Issue | Severity | Exploitable? | Threshold |
|---|-------|----------|---|---|
| 3 | Simulation doesn't test materialization + constraints | **High** | Test coverage gap | **Now** — addressed by pipeline assertions (PR #643) |
| 1 | Distance cutoff prunes connectivity scope | **Medium** | Attacker controls diversity via weight manipulation | **~500 users** with multi-community structure (3+ hops between communities) |
| 2 | CTE is exponential in dense subgraphs | **Medium** | DoS vector — dense cluster makes `compute_distances_from` expensive | **~1,000 users** with dense clusters (avg degree >10); a 15-node clique generates millions of CTE rows |
| 5 | Scenarios too structurally simple | **Medium** | Incomplete validation | Not a vulnerability — test coverage gap |
| 6 | Stale comments reference old approximation | **Low** | Misleading but not broken | **Now** — addressed by comment fixes (PR #643) |
| 4 | Cross-edge += relies on uniqueness constraint | **Low** | Only if schema invariant violated | **Never** if DB unique index holds; verify index exists |
| 8 | Double distance computation | **Low** | Performance only | Same as #2 (~1,000 dense users) — doubles an already-expensive operation |
| 7 | Deterministic hash produces structured graphs | **Low** | Weak test realism | Never a vulnerability |

### Scale analysis

The two findings that become real vulnerabilities pre-public-launch:

- **#2 (CTE DoS)** is the most dangerous. It's a **resource attack, not a trust attack** — an attacker doesn't need to be in the trust graph, just create a dense endorsement cluster to make the DB work hard. Fix: query timeout + max recursion depth on the CTE.
- **#1 (distance-connectivity coupling)** lets an attacker manipulate diversity scores by lengthening alternative paths past the 10.0 cutoff — effectively controlling whether someone passes a `min_diversity` constraint without touching the target's direct endorsements. Fix: decouple edge loading from distance reachability (load all edges, not just distance-reachable ones).

Both are safe at demo scale (~100 users, sparse graph). Must be addressed before public launch.

---

## Recommended Actions

### Addressed (PR #643)
- **#3**: Pipeline assertions added — `materialize()` + `check_eligibility()` on 3 adversarial topologies
- **#6**: Stale comments updated to reference vertex connectivity

### Pre-public-launch (post-demo)
- **#2**: Add a query timeout or max recursion depth to the distance CTE. Consider materialized-view approach for distance computation at scale.
- **#1**: Decouple distance reachability from connectivity scope. Load edges for *all* nodes in the DB, not just the distance-reachable set.
- **#4**: Verify unique index on `(endorser_id, subject_id, topic)` exists; add if missing.

### Backlog
- **#5**: Add multi-attachment, weight-exploitation, and graph-splitting scenarios
- **#7**: Replace linear hash with a seeded PRNG for more realistic topology generation
- **#8**: Deduplicate distance computation (resolved naturally if #2 is addressed)
