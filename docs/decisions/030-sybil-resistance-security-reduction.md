# ADR-030: Sybil Resistance Security Reduction — diversity = bridge_count

## Status
Accepted (2026-03-16)

## Context

The trust engine's diversity metric counts vertex-disjoint paths from the anchor to each user via Edmonds-Karp max-flow (ADR-019, as amended). The eligibility threshold requires diversity >= 2, meaning at least two independent paths from the anchor must exist. The question is: what does this threshold actually guarantee against a Sybil attacker?

The scale simulation framework (PR #684) tested Sybil mesh topologies across multiple bridge counts (1, 2, 3, and 5 bridges) to determine whether internal mesh endorsements (fake-to-fake) can inflate the diversity score.

### Simulation evidence

Sybil mesh topology: N fake nodes (tested with 20) forming a fully-connected internal mesh, attached to the legitimate graph through B bridge nodes on vertex-disjoint paths from the anchor.

| Bridges | Mesh diversity (all members) | Internal edges present | Diversity inflated? |
|---|---|---|---|
| 1 bridge | diversity = 1 | Yes (20 nodes, ~190 edges) | **No** |
| 2 bridges | diversity = 2 | Yes | **No** |
| 3 bridges | diversity = 3 | Yes | **No** |
| 5 bridges | diversity = 5 | Yes | **No** |

In every case, mesh members' diversity equals exactly the number of bridge nodes, regardless of how many internal fake-to-fake endorsements exist. A colluding ring of 6 nodes with circular endorsements also scored diversity = 1 (single attachment point), despite the dense internal connectivity.

The max-weight Sybil test (all internal edges at weight 1.0) confirmed: diversity = 1 regardless of weight magnitude. Weight affects distance but not the diversity metric's path-independence counting.

## Decision

### The core security claim: diversity = bridge_count.

Sybil mesh members achieve diversity exactly equal to the number of bridge nodes on vertex-disjoint paths from the anchor. Internal mesh endorsements cannot inflate diversity. This is a direct consequence of the vertex-disjoint path formulation: every independent path from anchor to a mesh node must pass through a distinct bridge node, and no amount of internal mesh connectivity can create a new path that does not pass through an existing bridge.

### Sybil resistance reduces to bridge compromise cost.

The practical security question becomes: "how hard is it to compromise 2+ accounts on genuinely independent paths from the anchor?" This is the attacker's minimum cost to get any Sybil identity past the diversity >= 2 threshold. The cost depends on:

- **Population size.** If willingness to sell/compromise an endorsement is normally distributed, the cheapest 2 accounts on independent paths get cheaper as N grows.
- **Attack vector.** Social engineering the endorsement ceremony (red team A3) requires physical copresence per bridge. Account takeover (A2) bypasses the ceremony entirely but requires credential compromise.
- **Slot budget constraint.** Each bridge can endorse at most k=10 mesh nodes directly (ADR-020). Mesh-internal endorsements can extend reach beyond direct bridge connections.

### What this reduction does NOT guarantee.

The mechanism math is sound — `diversity = bridge_count` holds at any scale. But the *cost* of compromising bridge accounts decreases with population. Account takeover (A2) bypasses the endorsement ceremony entirely: the attacker inherits an already-endorsed account's full graph position. The reduction proves the mechanism works; operational security (detection, response, heuristics) determines whether the mechanism is sufficient.

## Consequences

### Positive
- **Clean, auditable security claim.** The system's Sybil resistance can be stated precisely: "an attacker needs B compromised accounts on independent paths to create Sybil identities with diversity = B." No hand-waving about "approximately resistant."
- **Formally reasoned about.** The claim follows from Menger's theorem applied to the vertex-split graph construction in the max-flow computation. It is testable, falsifiable, and has been empirically verified across multiple mesh sizes and bridge counts.
- **Bounds the attacker's problem.** Internal mesh operations (creating fake accounts, fake-to-fake endorsements) do not help. Only bridge acquisition matters. This simplifies both the defender's mental model and the attacker's cost analysis.

### Negative
- **The per-bridge cost decreases with scale.** At 100k users, the tail of the willingness-to-compromise distribution gets cheaper. The diversity threshold sets *how many* bridges are needed but not the *per-unit cost*. This is addressed by heuristic detection (Tier 3+ work).
- **Account compromise bypasses the endorsement ceremony.** The reduction assumes bridges are acquired through the endorsement process. Account takeover skips this entirely — the compromised account IS a legitimate bridge. Detection requires behavioral/temporal analysis, not graph structure.
- **Single bridge count = N Sybils.** Once an attacker acquires B >= 2 bridges, they can create an arbitrarily large number of Sybil identities that all achieve diversity = B (bounded by k=10 direct endorsements per bridge, extended through mesh-internal paths). The slot budget limits per-bridge fanout but not mesh-internal propagation.

### Neutral
- The reduction composes with all other trust mechanisms: denouncement (ADR-024) can remove bridge edges, time decay (ADR-025) forces bridge renewal, slot budget (ADR-020) limits per-bridge fanout.
- The reduction is independent of edge weights — diversity counts paths, not their weights.

## References
- [ADR-019: Trust engine computation](019-trust-engine-computation.md) — vertex-disjoint max-flow (as amended by ADR-028)
- [ADR-020: Reputation scarcity](020-reputation-scarcity.md) — slot budget k=10 bounding bridge fanout
- [ADR-023: Fixed slots with variable weight](023-fixed-slot-variable-weight.md) — weight independence confirmed
- `.plan/2026-03-13-trust-robustness-overview.md` — "Key scale insight" section
- `.plan/2026-03-13-red-team-threat-model.md` — A4 (Sybil mesh with purchased bridges)
- `.plan/2026-03-13-scale-readiness-matrix.md` — Tier 1 gate: diversity = bridge_count proven
- PR #684: Scale simulation framework (Sybil mesh analysis across 1-5 bridge scenarios)
- PR #652: Diversity fix replacing endorser-count approximation with exact vertex connectivity
