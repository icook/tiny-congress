# ADR-027: Founder as Trust Anchor at Launch; Multi-Anchor Migration at Tier 3+

## Status
Accepted (2026-03-16)

## Context

The trust engine computes all scores relative to a single anchor node (ADR-019). ADR-026 established that this anchor is a necessary security property — anchor-free global algorithms (EigenTrust, PageRank) fail at Sybil detection. The anchor provides the symmetry-breaking reference point that prevents self-reinforcing trust loops from gaming eligibility.

This creates a structural question: who is the anchor, and what happens if the anchor is compromised?

At launch scale (demo, ~100 users), the platform creator is bootstrapping the trust graph from their personal network. Every trust network — PGP, BrightID, real-world social networks — starts from a founder's personal connections. The anchor is not "unearned structural privilege"; it is the inevitable shape of a new trust graph.

At scale (~10k+ users), the founder cannot personally vouch for the network's integrity. The structural advantage becomes politically significant, and anchor compromise (red team A5) becomes a distinct threat vector rather than a special case of account compromise (A2).

BrightID's experience validates this sequencing: attempting to decentralize trust governance before a user base exists creates more problems than it solves.

## Decision

### At launch, the founder's account is the trust anchor.

This is accepted and pragmatic. Concretely:

- Anchor = founder account. Anchor compromise = founder account compromise (red team A2, not a separate threat vector).
- The "single point of failure" risk is bounded by the small user base — at demo scale, the founder personally knows most participants.
- The genesis-drop concern (one node's position is structural, not earned) becomes real only if/when platform participation carries material value AND the founder's structural advantage persists without accountability. At demo scale, neither condition holds.

### Multi-anchor migration is deferred to Tier 3+ (~10k users).

When the network is large enough that the founder does not personally know most participants, the anchor should transition to a distributed model. The solution space (evaluated but not selected):

1. **Multi-anchor with consensus.** Compute anchor-relative scores from N independent anchors. Require agreement (intersection, minimum, or weighted combination). Preserves the `diversity = bridge_count` security reduction while distributing the point of failure. Open questions: anchor selection, disagreement handling, quorum size.

2. **Personalized PageRank (PPR).** PPR computes PageRank relative to a specific "teleport" node — essentially anchor-relative PageRank. Used by SybilRank for Sybil detection. Retains the anchor's symmetry-breaking property while computing a smoother score than shortest-path distance. Open question: whether this offers genuinely different properties or is just "better anchor-relative scoring."

3. **Distributed / rotating anchor.** Keep anchor-relative scoring but make the anchor a governance role: rotatable, electable, or distributed. Open questions: score stability during rotation, migration path when anchor changes.

4. **Community detection hybrid.** Use graph clustering (Louvain, spectral) to identify communities, then detect the "attack edge" between real communities and Sybil clusters as a supplementary signal. Open question: whether community detection can replace the anchor or is only a heuristic.

### Identity verification is a separate concern.

The anchor problem applies to the trust graph — who vouches for whom. Identity verification (proving unique humanity) is orthogonal: it is a node property (verified by which verifiers), not a graph edge. Verifiers confirm facts; they do not have a trust relationship. See ADR-031 for how verifiers participate in the graph.

## Consequences

### Positive
- **Unblocks launch.** No need to solve the multi-anchor problem before shipping the demo.
- **Honest about bootstrapping.** Acknowledges that every trust network starts from a founder, rather than pretending otherwise.
- **Bounded risk.** At demo scale, the attack surface is small enough that mechanism security alone is sufficient — operational security is not yet needed.
- **Clear migration trigger.** The transition criterion is concrete: when the founder does not personally know most participants (~10k users).

### Negative
- **Single point of failure persists.** Founder account compromise at launch = total trust graph compromise. Mitigated by: the founder presumably has strong key management, and at demo scale the impact is bounded.
- **No rotation procedure exists.** If the founder needs to be replaced (voluntary or involuntary), there is no defined process. This is acceptable at launch but becomes a gap at scale.
- **Philosophical tension.** "The server is a dumb witness, not a trusted authority" — but the anchor IS the ultimate trusted authority. This tension is acknowledged, not resolved. Resolution requires the multi-anchor migration.

### Neutral
- The anchor question reframes from "remove the anchor" (impossible per ADR-026) to "make the anchor more accountable and redundant" — a governance problem, not an algorithm problem.
- At demo scale, anchor = founder is operationally simpler than any distributed alternative.

## References
- [ADR-019: Trust engine computation](019-trust-engine-computation.md) — anchor-relative scoring mechanism
- [ADR-026: Anchor-relative scoring](026-anchor-relative-scoring.md) — why anchor-free alternatives fail
- [ADR-017: Two-layer trust architecture](017-two-layer-trust-architecture.md) — platform trust layer that the anchor serves
- `.plan/2026-03-13-anchor-problem-statement.md` — launch decision, solution space, and BrightID precedent
- `.plan/2026-03-13-open-questions.md` — Q28 (anchor redundancy), Q29 (anchor-free scoring)
- `.plan/2026-03-13-red-team-threat-model.md` — A5 (anchor compromise)
- `.plan/2026-03-13-scale-readiness-matrix.md` — Tier 3 gate: multi-anchor migration evaluated
