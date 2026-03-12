# ADR-017: Trust Anchor Bootstrap via Engine-Level Injection

## Status
Accepted

## Context
The trust engine's recursive CTE (`compute_distances_from`) computes minimum weighted hop-count distances from an anchor to every reachable user. However, the CTE only traverses outward edges — it never produces a row for the anchor itself. Without distance=0 for the anchor, the trust score display pipeline has no root to anchor against.

## Decision
Inject the anchor score at the engine level. After the CTE query returns, `compute_distances_from` prepends `(anchor_id, distance=0.0)` into the result set. The anchor's path diversity is pinned to `i32::MAX` in `recompute_from_anchor` since the anchor is the root of trust, not a node requiring independent verification.

## Consequences

### Positive
- One-liner in the right place — no schema changes, no CTE modifications
- Anchor score is always present regardless of graph shape
- Idempotent: batch recomputation re-injects the anchor every time

### Negative
- Callers of `compute_distances_from` must be aware the anchor is included in results (minor — this is the expected behavior)

### Neutral
- `recompute_from_anchor` return count now includes the anchor (tests updated accordingly)

## Alternatives considered

### A: Convention row in trust__score_snapshots
- Seed a distance=0 row during account creation
- Rejected: fragile — batch recomputation could overwrite or miss it

### B: Self-endorsement
- Anchor endorses itself to produce a CTE row
- Rejected: semantically wrong, consumes an endorsement slot, produces distance=1.0 not 0.0

### D: Genesis endorsement (NULL endorser)
- Special CTE base case for NULL endorser_id
- Rejected: requires CTE modification and special NULL handling in endorsement queries

## References
- Issue #638
- PR #641
