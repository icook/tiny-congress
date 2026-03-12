# ADR-019: Trust Engine — Distance and Diversity Computation

## Status
Accepted

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

**Approximation (not exact max-flow):** For each target user, count the number of distinct active endorsers who are themselves reachable from the anchor:

```sql
SELECT
    e.subject_id AS user_id,
    COUNT(DISTINCT e.endorser_id)::int AS path_diversity
FROM reputation__endorsements e
WHERE e.revoked_at IS NULL
  AND e.endorser_id IS NOT NULL
  AND e.endorser_id = ANY(:reachable_user_ids)
GROUP BY e.subject_id;
```

This is a two-step process:
1. Run `compute_distances_from(anchor)` to get the reachable set.
2. For each user, count distinct endorsers within that reachable set.

**Sybil-resistance property:** A hub-and-spoke attacker who routes N endorsements through a single node gives each target `diversity = 1`. A user endorsed by K independent reachable nodes gets `diversity = K`. This is intentionally an approximation — true edge-disjoint path counting requires max-flow computation, which is prohibitive at scale and unnecessary at demo scale.

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
- The diversity approximation is O(E) where E is the edge count in the reachable subgraph — tractable for demo scale.
- Materialized scores make room eligibility checks a simple table lookup.

### Negative
- The recursive CTE explores all paths up to distance 10.0, which can be expensive on dense graphs. The 10.0 cutoff is a tuning parameter that trades accuracy for performance.
- The diversity approximation can overcount in some topologies (a user with two endorsers who share an upstream path gets diversity 2, even though the paths aren't truly independent). For demo scale this is acceptable.
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

### Exact edge-disjoint paths (max-flow)
- Computes the true number of independent paths between two nodes.
- Rejected for MVP due to computational cost (max-flow is O(V * E^2) in the general case). The distinct-endorser approximation provides adequate Sybil resistance at demo scale.

## References
- [ADR-008: Account-based verifiers](008-account-based-verifiers.md) — verifier endorsements populate the same table the CTE traverses
- [ADR-017: Two-layer trust architecture](017-two-layer-trust-architecture.md) — the trust engine serves the platform trust layer
- [ADR-018: Handshake protocol](018-handshake-protocol.md) — how edges are created and weighted
- [ADR-020: Reputation scarcity](020-reputation-scarcity.md) — trust scores may influence slot allocation
- [ADR-021: Batch reconciliation](021-batch-reconciliation.md) — when scores are recomputed
- `service/src/trust/engine.rs` — `compute_distances_from`, `compute_diversity_from`, `recompute_from_anchor`
- `service/src/trust/repo/scores.rs` — `upsert_score`, `get_score`, `get_all_scores`
- TRD §3 (The Trust Engine) — original specification
