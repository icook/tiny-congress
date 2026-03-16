# ADR-031: Identity Verification as Graph Participation

## Status
Accepted (2026-03-16)

## Context

The trust graph carries an implicit identity signal: when a trusted person endorses someone via QR handshake, they are attesting to both humanity and social trust. As the system grows, external identity verifiers (BrightID, passport services, trusted community organizers at events) will need to participate. The question is: how do verifiers integrate with the trust model?

Two conceptual operations are at play:

- **Identity verification** — proving someone is a unique human. A fact about the person, not a relationship.
- **Trust endorsement** — vouching for someone. A relationship between two people.

At launch, the QR handshake implicitly conflates both. Separating them becomes necessary before adding external identity providers.

### The slot budget problem

ADR-020 establishes k=10 endorsement slots as a core Sybil defense. A verifier entity that needs to endorse thousands of verified humans cannot operate within k=10. Four options were evaluated:

| Option | Mechanism complexity | Blast radius if compromised | Scales to 10k verified? | Special entity types? |
|---|---|---|---|---|
| A. Variable k per entity | Low (one parameter) | High (k=infinity node) | Yes | Yes |
| B. Light endorsement tier | Medium (new edge tier) | Low (low-weight edges) | Yes | No |
| C. Multiple k=10 instances | None | Low (k=10 per instance) | Awkward (1000 instances for 10k) | No |
| D. Room endorsement | Low (reuse rooms) | Medium (room entity) | Yes | Sort of |

## Decision

### Verifiers participate as entities in the trust graph, not as a separate system.

The founder endorses verifier entities with high trust. Verifiers endorse verified humans. Those humans get graph position through the verifier's path from anchor. This is architecturally clean: the security model is unchanged (`diversity = bridge_count` still holds per ADR-030), a compromised verifier only affects paths through that verifier, and the graph does not need to know what a "verifier" is — it just sees edges.

### Identity verification and trust endorsement are distinct operations.

Verifiers confirm facts about a person (unique humanity). Endorsers confirm trust in a person (social vouching). These are different signals with different semantics. The trust graph carries both, but the distinction matters for: weight assignment (verification may warrant a different weight than personal trust), blast radius (a compromised verifier affects identity claims, not personal trust relationships), and future audit (which edges represent personal judgment vs. institutional verification).

### The slot budget problem is deferred; light endorsement tier is the leading option.

Option B (light endorsement tier) is the most aligned with existing design principles: no special entity types, general mechanism available to all users, bounded blast radius from low-weight edges. Everyone would get k=10 full endorsements plus k=N light endorsements (lower weight, e.g., 0.1-0.2). Verifiers use the light tier. This is the same "additional endorsement tiers" concept described in the trust expansion analysis.

Option C (multiple k=10 instances) works naturally when verifiers are humans performing a role at community events — no mechanism changes required, just operational scaling.

The choice depends on whether verifiers are primarily automated services (favors B) or humans performing a role (favors C). This does not need to be resolved before launch because the founder's personal network covers the initial user base.

## Consequences

### Positive
- **No separate verification layer.** Verifiers use the same graph infrastructure as everyone else. No new tables, no new trust computation, no new API surface.
- **Security model preserved.** A compromised verifier is just a compromised node — `diversity = bridge_count` still applies. Users verified through a single compromised verifier have diversity contribution only through that verifier's paths.
- **Graceful degradation.** If a verifier is revoked, only paths through that verifier are affected. Users with endorsements from other sources retain their graph position.
- **General mechanism.** The light endorsement tier (if chosen) is available to all users, not just verifiers — it also addresses the diversity problem for tight communities where all paths funnel through one bridge.

### Negative
- **Deferred implementation.** The slot budget solution is identified but not built. This is acceptable at launch scale but becomes a blocker before adding external identity providers.
- **Conflation at launch.** The QR handshake still conflates identity verification and trust endorsement. Users cannot distinguish "this person is verified as human" from "this person is trusted by their endorser." Acceptable for demo but needs separation before scale.
- **Light endorsement tier adds complexity.** Two tiers of endorsement (full and light) with different weights and potentially different slot budgets increases the mechanism surface area.

### Neutral
- ADR-008 (account-based verifiers) already establishes verifier accounts. This ADR clarifies how those accounts participate in the trust graph specifically.
- The existing weight table (ADR-023) already supports low weights (email = 0.1). A light endorsement tier may be primarily a UX change rather than a mechanism change.

## References
- [ADR-008: Account-based verifiers](008-account-based-verifiers.md) — verifier account concept
- [ADR-017: Two-layer trust architecture](017-two-layer-trust-architecture.md) — platform vs. room trust layers
- [ADR-020: Reputation scarcity](020-reputation-scarcity.md) — slot budget (k=10) and verifier exemption
- [ADR-030: Sybil resistance security reduction](030-sybil-resistance-security-reduction.md) — diversity = bridge_count holds regardless of verifier design
- `.plan/2026-03-13-open-questions.md` — Q30 (full analysis with 4 options and trade-off table)
- `.plan/2026-03-13-anchor-problem-statement.md` — identity verification as separate concern
- `.plan/2026-03-13-trust-expansion-concepts.md` — additional endorsement tiers concept
