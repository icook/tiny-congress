# The Anchor Problem: Sybil-Resistant Trust Without a Single Root

**Date:** 2026-03-13 (revised 2026-03-15)
**Status:** Launch decision made — founder is the trust root. Multi-anchor is a Tier 3+ scale concern.
**Context:** Spike results from EigenTrust/PageRank comparison (branch `test/680-scale-simulation-framework`, worktree `eigentrust-spike`). Launch framing from trust architecture review (2026-03-15).

---

## Problem statement

We are building a web-of-trust system for community governance. Users endorse each other through in-person key exchange ceremonies (QR code swap). The trust graph determines platform eligibility: who can vote, participate in governance, and access trust-gated features.

**The current system uses anchor-relative scoring.** All trust is measured from a single "anchor" node — the root of the trust graph. Two metrics:

1. **Distance** — weighted shortest path from anchor to user (Dijkstra, cost = 1/edge_weight)
2. **Diversity** — number of vertex-disjoint paths from anchor to user (Edmonds-Karp max-flow)

Eligibility requires: distance ≤ 5.0 AND diversity ≥ 2.

**This works extremely well for Sybil detection.** We have a proven security reduction: `diversity = bridge_count` — the number of genuinely independent accounts an attacker must compromise to get a Sybil identity past the diversity threshold. Internal Sybil mesh endorsements (fake-to-fake) do not inflate diversity. The mechanism is clean, auditable, and formally reasoned about.

**But the anchor is a single point of failure and a structural privilege.** The anchor's trust position is axiomatic — they are trusted by definition, not by endorsement. Every other node's civic standing derives from proximity to this one person. This creates:

- **Red team A5:** anchor compromise = total system failure. No detection mechanism exists.
- **No rotation procedure:** how do you change the anchor? What happens to all scores?
- **"Genesis drop" problem:** in a system that values earned trust, one node's position is structural, not meritocratic.
- **Philosophical contradiction:** "the server is a dumb witness, not a trusted authority" — but the anchor IS the ultimate trusted authority.

## Launch decision (2026-03-15)

**The founder is the trust root at launch. This is accepted and pragmatic.**

The genesis drop concern is real but dissolves under scrutiny at launch scale: the person bootstrapping the network IS the trust root by definition. Every trust network — PGP, BrightID, real-world social networks — starts from a founder's personal network. The anchor isn't "unearned structural privilege"; it's the inevitable shape of a new trust graph. BrightID's experience validates this: trying to decentralize trust governance before a user base exists creates more problems than it solves.

**What this means concretely:**
- At launch, anchor = founder account. Anchor compromise = founder account compromise (red team A2, not a separate threat vector).
- The "single point of failure" risk is bounded by the small user base — at demo scale, the founder personally knows most participants.
- The genesis drop concern becomes real only if/when platform participation carries material value AND the founder's structural advantage persists without accountability. At demo scale, neither condition holds.

**Migration path (Tier 3+, ~10k users):**
When the network is large enough that the founder doesn't personally know most participants, the anchor should transition to a distributed model. The solution space below (multi-anchor, rotating anchor, community-elected anchors) becomes relevant at that scale. This is future work — important but not blocking.

**Identity verification is a separate concern.** The anchor problem applies to the **trust graph** — who vouches for whom. Identity verification (proving you're a unique human) is orthogonal: it's a **node property** (verified by: [list of verifiers]), not a graph edge. Verifiers confirm facts about you; they don't have a trust relationship with you. The trust graph measures social trust, not identity. See open question Q30.

## What we tested (and why it failed)

**Spike: EigenTrust and PageRank as anchor-free alternatives.**

Both algorithms compute global trust/reputation scores from graph structure alone, without a distinguished root node. We implemented both and tested against adversarial topologies.

### Results

| Scenario | Anchor-relative separation | EigenTrust/PageRank separation |
|---|---|---|
| Sybil mesh, 1 bridge (20 fake nodes) | **1.36x** (blue closer to anchor) | **0.95x** (Sybils score HIGHER) |
| Sybil mesh, 2 bridges | **1.36x** | **0.94x** (Sybils score HIGHER) |
| Sybil mesh, 5 bridges | — | **0.83x** (Sybils score 1.21x HIGHER) |
| Colluding ring (6 nodes, circular endorsements) | **5.4x** | **0.68x** (ring inflates scores 1.46x) |

**EigenTrust alpha sensitivity** (2-bridge Sybil, varying pretrust blend):

| Alpha | Separation (blue/red) | Interpretation |
|---|---|---|
| 0.05 (graph-driven) | 0.78x (Sybils win) | More graph influence = worse Sybil detection |
| 0.15 | 0.94x (Sybils win) | Default EigenTrust parameter — still fails |
| 0.30 | 1.01x (near parity) | Pretrust starts dominating |
| 0.50 (pretrust-driven) | 1.04x (slight blue edge) | Algorithm adds almost no value over uniform |

### Why anchor-free scoring fails at Sybil detection

**The fundamental problem:** EigenTrust and PageRank answer "how connected are you to well-connected nodes?" Sybil meshes are *by construction* densely connected — every fake node endorses every other fake node. The algorithms cannot distinguish "endorsed by many real people" from "endorsed by many fake people endorsing each other."

**Circular endorsement amplification:** A colluding ring (A→B→C→D→E→F→A) creates a trust feedback loop. Each iteration of EigenTrust/PageRank amplifies the scores of ring members. In our test, ring members scored **1.46x higher** than legitimate nodes.

**The anchor breaks this symmetry.** Anchor-relative scoring works because trust must *originate* from the anchor and *flow outward* through real endorsements. Self-reinforcing loops don't help because they don't create new paths from the anchor. The anchor provides an external reference point that circular structures cannot game.

**The alpha dilemma:** EigenTrust's pretrust parameter controls how much weight is given to the trusted seed set vs. the graph structure. Low alpha (graph-driven) lets Sybils exploit dense connectivity. High alpha (pretrust-driven) converges to uniform scores, adding no value. There is no sweet spot where the graph structure alone correctly identifies Sybils without a trusted reference.

## The design tension (reframed)

We need:
1. **Sybil resistance** — which requires a trusted reference point (anchor) to break self-reinforcement symmetry
2. **No single point of failure** — which requires eliminating or distributing the anchor
3. **Earned trust** — which requires the anchor's position to be accountable, not structural

At launch, (1) is satisfied by the founder-as-anchor. (2) and (3) are acceptable trade-offs at demo scale — the founder's trust position is structural but transparent and acknowledged. The tension becomes real at scale (~10k+), where the founder can't personally vouch for the network's integrity and the structural advantage becomes politically significant.

The question for later: can we design a migration from single-anchor to distributed trust that preserves the security reduction while addressing (2) and (3)?

## Solution space (for Tier 3+ migration)

### A. Multi-anchor with consensus

Compute anchor-relative scores from N independent anchors. Require agreement (intersection, minimum, or weighted combination). Preserves the security reduction while distributing the point of failure.

**Open questions:** How are anchors selected? Can anchors be community-elected? What happens when anchors disagree? How many anchors are needed for meaningful redundancy? Does this create an anchor oligarchy?

### B. Personalized PageRank (PPR)

PPR computes PageRank relative to a specific "teleport" node — essentially anchor-relative PageRank. It's been used for Sybil detection (SybilRank). Unlike pure PageRank, it retains the anchor's symmetry-breaking property while computing a smoother score than shortest-path distance.

**Open question:** Is PPR just "better anchor-relative scoring" or does it offer genuinely different properties? Does it address the genesis drop concern at all?

### C. Community detection + local trust

Use graph clustering (Louvain, spectral, etc.) to identify communities, then compute trust within and between communities. The "attack edge" between the real community and a Sybil cluster is structurally detectable without needing an anchor.

**Open question:** Can community detection replace the anchor, or is it only a supplementary heuristic? What are the false positive rates?

### D. Distributed / rotating anchor

Keep anchor-relative scoring but make the anchor position rotatable, electable, or distributed. The anchor is a governance role, not a permanent identity.

**Open question:** How does anchor rotation affect score stability? What's the migration path when the anchor changes?

### E. Proof-of-Personhood integration

Use external Sybil resistance (BrightID, passport verification, etc.) as a bootstrap, reducing the trust graph's burden from "prove you're real" to "prove you're trusted."

**Open question:** Does this just move the single-point-of-failure to the PoP provider?

### F. Hybrid: anchor-relative for eligibility, global for anomaly detection

Accept that anchor-relative scoring is necessary for Sybil-resistant eligibility, but use EigenTrust/PageRank as supplementary anomaly detection signals (sudden score changes = topology shifts). Address the genesis drop concern through governance (anchor rotation, multi-anchor) rather than algorithm replacement.

**Key realization from spike:** EigenTrust/PageRank may be more valuable as *anomaly detectors* than as *eligibility scorers*. A sudden change in a node's PageRank indicates a topology shift — useful for detecting compromised accounts or Sybil cluster formation — even if the absolute score doesn't reliably separate real from fake.

## What we're looking for (when migration becomes relevant)

Any approach that achieves Sybil resistance in a trust graph without requiring a single permanent root of trust. Specifically:

- Systems that have been deployed at scale (>10k users)
- Formal security reductions (what must an attacker compromise?)
- Approaches that handle dense Sybil meshes (not just isolated Sybil nodes)
- Mechanisms for selecting/rotating/distributing the trusted seed set
- Trade-off analyses between anchor-free simplicity and Sybil resistance

## Constraints

- The trust graph is built from in-person endorsement ceremonies (QR code swap). This is a strong identity signal but expensive to acquire.
- Endorsement slots are limited (k=10 per user). This bounds the attack surface but also limits legitimate connectivity.
- The system must work at 100k users. Algorithms must be computationally feasible at this scale.
- The mechanism design (slots, denouncement, decay) is orthogonal to scoring — any scoring system must compose with these.
- We are a small team. Operational complexity of the scoring system matters.
