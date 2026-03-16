# ADR-026: Anchor-Relative Scoring for Trust Eligibility; Global Algorithms Rejected

## Status
Accepted (2026-03-16)

## Context

The trust engine uses anchor-relative scoring: all trust is measured from a single "anchor" node via two metrics — distance (weighted shortest path via Dijkstra) and diversity (vertex-disjoint path count via Edmonds-Karp max-flow). This design has a clean security reduction (`diversity = bridge_count`) but creates structural dependence on the anchor.

The question was whether anchor-free global algorithms — EigenTrust and PageRank — could replace or supplement anchor-relative scoring for trust eligibility. Both compute reputation from graph structure alone, with no distinguished root node. If viable, they would eliminate the single-point-of-failure concern inherent to any anchor-based design.

A spike (2026-03-13, branch `test/680-scale-simulation-framework`) implemented both algorithms and tested them against adversarial topologies using the simulation harness.

### Spike results

| Scenario | Anchor-relative separation | EigenTrust/PageRank separation |
|---|---|---|
| Sybil mesh, 1 bridge (20 fake nodes) | **1.36x** (blue closer to anchor) | **0.95x** (Sybils score HIGHER) |
| Sybil mesh, 2 bridges | **1.36x** | **0.94x** (Sybils score HIGHER) |
| Sybil mesh, 5 bridges | — | **0.83x** (Sybils score 1.21x HIGHER) |
| Colluding ring (6 nodes, circular endorsements) | **5.4x** | **0.68x** (ring inflates scores 1.46x) |

EigenTrust alpha sensitivity (2-bridge Sybil, varying pretrust blend):

| Alpha | Separation (blue/red) | Interpretation |
|---|---|---|
| 0.05 (graph-driven) | 0.78x (Sybils win) | More graph influence = worse Sybil detection |
| 0.15 (default) | 0.94x (Sybils win) | Default EigenTrust parameter — still fails |
| 0.30 | 1.01x (near parity) | Pretrust starts dominating |
| 0.50 (pretrust-driven) | 1.04x (slight blue edge) | Algorithm adds almost no value over uniform |

## Decision

### Anchor-free global scoring cannot be used for trust eligibility.

EigenTrust and PageRank answer "how connected are you to well-connected nodes?" Sybil meshes are by construction densely connected — every fake node endorses every other fake node. The algorithms cannot distinguish "endorsed by many real people" from "endorsed by many fake people endorsing each other."

**Circular endorsement amplification:** A colluding ring (A->B->C->D->E->F->A) creates a trust feedback loop. Each iteration amplifies ring members' scores. In testing, ring members scored 1.46x higher than legitimate nodes.

**The alpha dilemma:** EigenTrust's pretrust parameter controls weight given to the trusted seed set vs. graph structure. Low alpha (graph-driven) lets Sybils exploit dense connectivity. High alpha (pretrust-driven) converges to uniform scores, adding no value. There is no sweet spot where graph structure alone correctly identifies Sybils without a trusted reference.

**The anchor breaks this symmetry.** Anchor-relative scoring works because trust must originate from the anchor and flow outward through real endorsements. Self-reinforcing loops do not create new paths from the anchor. The anchor provides an external reference point that circular structures cannot game.

### EigenTrust/PageRank retain value ONLY for anomaly detection.

The spike revealed a valid use case: topology shift monitoring. A sudden change in a node's PageRank indicates a topology shift — useful for detecting compromised accounts or Sybil cluster formation — even if the absolute score does not reliably separate real from fake. Relative changes over time are meaningful; absolute values are not.

Recommended use: nightly batch anomaly detection, not eligibility scoring.

### Computational feasibility is not the constraint.

At 100k users with k=10 endorsement slots, the entire graph is ~16 MB. EigenTrust (258ms at 1k nodes) and PageRank (316ms at 1k nodes) are trivially fast. The problem is not performance — it is that the algorithms produce the wrong answer for Sybil detection.

## Consequences

### Positive
- **Eliminates a class of design mistakes.** Future proposals to "add PageRank for eligibility" can be rejected by reference to this ADR and the empirical data.
- **Clarifies the anchor's role.** The anchor is not an arbitrary design choice — it provides a security property (symmetry-breaking) that global algorithms cannot replicate.
- **Preserves anomaly detection as a valid future capability.** Reframing EigenTrust/PageRank as anomaly detectors (not eligibility scorers) keeps them in the toolkit for operational security at scale.

### Negative
- **The anchor remains a single point of failure.** This ADR accepts that anchor-relative scoring is necessary, which means the genesis-drop concern and anchor compromise risk (red team A5) persist. These are addressed by ADR-027 (founder as anchor at launch, multi-anchor migration at scale).
- **Anomaly detection is deferred work.** The valid use case (topology shift monitoring) is identified but not implemented.

### Neutral
- The spike's negative result is itself valuable — it empirically constrains the solution space for future trust scoring proposals.
- EigenTrust/PageRank may compose with anchor-relative scoring as supplementary signals for graph health monitoring (Tier 3+ work).

## References
- [ADR-019: Trust engine computation](019-trust-engine-computation.md) — anchor-relative distance and diversity
- [ADR-027: Founder as trust anchor](027-founder-trust-anchor.md) — addresses the anchor's single-point-of-failure
- `.plan/2026-03-13-anchor-problem-statement.md` — full spike analysis with data tables and root cause
- `.plan/2026-03-13-trust-expansion-concepts.md` — reframing of EigenTrust/PageRank as anomaly detection
- PR #684: Scale simulation framework (branch `test/680-scale-simulation-framework`)
