# ADR-019: Trust Engine — Distance and Diversity Computation

## Status
Accepted

**Amended 2026-03-16 (ADR-028):** Path diversity computation replaced. The original `COUNT(DISTINCT endorser_id)` approximation has been superseded by exact vertex-disjoint path count via Edmonds-Karp max-flow (PR #652). The approximation was exploitable: a colluding ring of 6 nodes scored diversity=6 under the old formula but diversity=1 under exact vertex connectivity. The `diversity = bridge_count` security reduction (ADR-030, proven in scale simulation PR #684) depends on the exact max-flow formulation.

## Context

The trust graph needs two quantitative metrics to gate participation: a measure of *how close* someone is to known-trusted identities (depth), and a measure of *how independently verified* they are (width). Together these form the composite trust score.

A single metric is insufficient:
- Distance alone is gameable: one compromised high-trust user can endorse an attacker at distance 1.
- Diversity alone doesn't capture trust quality: many low-weight referrals could create high diversity without meaningful verification.

The computation must run in PostgreSQL (where the graph data lives), handle cycles, respect edge weights, and terminate in bounded time.

## Decision

### Trust Distance (Depth)

Trust distance is the minimum weighted hop-count from a designated **trust anchor** (seed node) to a given identity. It answers: "how far is this person from someone we definitively trust?"

The computation uses a PostgreSQL recursive CTE:

```sql
WITH RECURSIVE trust_graph AS (
    SELECT
        e.subject_id AS user_id,
        (1.0 / e.weight)::real AS distance,
        ARRAY[e.endorser_id, e.subject_id] AS path
    FROM reputation__endorsements e
    WHERE e.endorser_id = :anchor_id
      AND e.revoked_at IS NULL
      AND e.endorser_id IS NOT NULL

    UNION ALL

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
GROUP BY user_id;
```

**Key properties:**
- **Cost function:** Each edge with weight `w` contributes `1/w` to accumulated distance. A physical QR edge (weight 1.0) costs 1.0. A social referral (weight 0.3) costs 3.33. Lower-quality verification produces longer paths.
- **Cutoff:** Traversal stops at accumulated distance 10.0, bounding computation time.
- **Cycle prevention:** The `path` array tracks visited nodes; `NOT (e.subject_id = ANY(tg.path))` prevents revisiting.
- **Shortest path wins:** `MIN(distance)` across all discovered paths to each user.
- **Direction:** Traversal follows edges outward from the anchor. Alice's endorsement of Bob makes Bob reachable from Alice, not the reverse.

### Path Diversity (Width)

Path diversity measures how many independent social circles vouch for a person. It answers: "if one branch of the trust graph is compromised, does this person still have a path to the anchor?"

**Exact vertex-disjoint path count via Edmonds-Karp max-flow.** For each target user, compute the maximum number of vertex-disjoint paths from the anchor using the standard vertex-split construction: each node (except source and sink) is split into an in-node and out-node connected by a capacity-1 edge. Edmonds-Karp finds the maximum flow, which equals the number of vertex-disjoint paths by Menger's theorem.

This is computed in-memory by `FlowGraph` in the trust engine:
1. Build the flow graph from all active (non-revoked) endorsement edges.
2. Apply vertex splitting so that each intermediate node can appear on at most one path.
3. Run Edmonds-Karp to find max-flow from anchor to each target.

**Sybil-resistance property:** The vertex-disjoint formulation provides the `diversity = bridge_count` security reduction (ADR-030). A Sybil mesh of N fake nodes with dense internal endorsements achieves diversity exactly equal to the number of bridge nodes on independent paths from anchor — internal mesh endorsements cannot inflate the count. A colluding ring of 6 nodes with a single attachment point scores diversity=1 regardless of internal connectivity.

**Superseded approximation:** The original implementation used `COUNT(DISTINCT endorser_id)` — counting distinct active endorsers reachable from the anchor. This approximation was exploitable: a colluding ring of 6 nodes where each endorses the next scored diversity=6 under the old formula (6 distinct endorsers) but diversity=1 under exact vertex connectivity (single attachment point). Replaced in PR #652 (fixes #648).

### Composite Trust Score

The trust score is the tuple `(trust_distance, path_diversity)`. These are stored in `trust__score_snapshots` with both a `context_user_id` (anchor-relative scores) and a global variant (`context_user_id IS NULL`).

Room gating policies evaluate thresholds against these values. Example configurations:

| Policy | Distance | Diversity | Intent |
|--------|----------|-----------|--------|
| Any verified human | any | >= 1 | Open community discussion |
| Community | <= 5.0 | >= 2 | Trusted community discussion |
| Congress | any | >= 3 | High-trust deliberation |

These thresholds are per-room configuration, not platform constants (see ADR-017).

### Materialization

Running the recursive CTE on every page load is prohibitive. Trust scores are materialized into `trust__score_snapshots` and recomputed during the daily batch reconciliation (see ADR-021).

The current implementation recomputes from the actor's anchor after each action (`recompute_from_anchor`). Under the batch model, the reconciliation job will recompute from all anchors against the full updated graph.

### Eigenvector centrality

A placeholder exists for eigenvector centrality as a global (non-anchor-relative) metric. This is stubbed (`recompute_global()` returns `Ok(0)`) and deferred to post-demo. It would measure a node's importance based on the importance of its endorsers — useful for identifying bridge nodes and detecting structural anomalies, but not needed for basic room gating.

## Consequences

### Positive
- The CTE runs entirely in PostgreSQL — no external graph database needed.
- The `1/weight` cost function creates a natural penalty for low-quality edges. Social referral chains are expensive; physical QR chains are cheap.
- The exact vertex-disjoint path count provides the `diversity = bridge_count` security reduction — a clean, auditable Sybil resistance claim (ADR-030).
- Materialized scores make room eligibility checks a simple table lookup.

### Negative
- The recursive CTE explores all paths up to distance 10.0, which can be expensive on dense graphs. The 10.0 cutoff is a tuning parameter that trades accuracy for performance.
- The `FlowGraph` for max-flow uses a dense O(n^2) adjacency matrix. At 1k nodes this is ~4MB; at 5k it hits ~100MB. A sparse Edmonds-Karp implementation has been proven equivalent in simulation tests and needs to be ported to the engine (#681).
- Anchor-scoped recomputation means Alice's view of the graph may disagree with Bob's view until both anchors are recomputed. The batch model resolves this by recomputing all anchors together.

### Neutral
- `ComputedScore` carries `path_diversity: i32` but this is only populated by the separate `compute_diversity_from` call, not by the distance CTE itself.
- The `trust__score_snapshots` table supports both anchor-relative and global scores via the nullable `context_user_id` column.

## Alternatives considered

### External graph database (Neo4j, Memgraph)
- Purpose-built for graph traversal, better query language for path analysis.
- Rejected because it adds operational complexity (another service to deploy, monitor, backup) and the CTE is sufficient for the expected graph size (hundreds to low thousands of nodes). Revisit if the graph exceeds ~100K nodes.

### Application-level graph traversal (Rust BFS/Dijkstra)
- Load the graph into memory, compute paths in Rust.
- Rejected because it duplicates state (graph in both DB and memory), requires synchronization, and the PostgreSQL CTE already expresses the algorithm correctly.

### Distinct-endorser count approximation (superseded)
- The original diversity implementation: `COUNT(DISTINCT endorser_id)` for each user's active endorsers reachable from anchor.
- Superseded by exact vertex-disjoint max-flow in PR #652. The approximation was exploitable: colluding rings and dense Sybil clusters scored artificially high diversity because distinct endorsers do not imply distinct paths. The security reduction `diversity = bridge_count` (ADR-030) requires the exact formulation.

## References
- [ADR-008: Account-based verifiers](008-account-based-verifiers.md) — verifier endorsements populate the same table the CTE traverses
- [ADR-017: Two-layer trust architecture](017-two-layer-trust-architecture.md) — the trust engine serves the platform trust layer
- [ADR-018: Handshake protocol](018-handshake-protocol.md) — how edges are created and weighted
- [ADR-020: Reputation scarcity](020-reputation-scarcity.md) — trust scores may influence slot allocation
- [ADR-021: Batch reconciliation](021-batch-reconciliation.md) — when scores are recomputed
- [ADR-030: Sybil resistance security reduction](030-sybil-resistance-security-reduction.md) — `diversity = bridge_count` proven from the vertex-disjoint formulation
- `service/src/trust/engine.rs` — `compute_distances_from`, `compute_diversity_from`, `recompute_from_anchor`
- `service/src/trust/repo/scores.rs` — `upsert_score`, `get_score`, `get_all_scores`
- PR #652: Replaced endorser-count approximation with exact vertex connectivity (fixes #648)
- PR #684: Scale simulation proving `diversity = bridge_count` across 1-5 bridge scenarios
- TRD §3 (The Trust Engine) — original specification
