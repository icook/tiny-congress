# ADR-020: Reputation Scarcity — Endorsement Slots and Action Budgets

## Status
Proposed

## Context

The trust graph needs a scarcity mechanism. Without one, a single user could endorse an unlimited number of identities, flooding the graph with low-quality trust edges and undermining Sybil resistance. The question is: what form should scarcity take?

Two models were considered:

**Continuous influence budget** (current backend implementation): Users have a float-valued influence pool (default 10.0). Endorsements "stake" influence, denouncements "spend" it. Available influence = total - staked - spent. This is flexible but hard for users to reason about.

**Discrete endorsement slots** (TRD design): Users have a fixed number of slots (k). Each active endorsement occupies one slot. To endorse a new person when full, you must revoke an existing endorsement. This is immediately legible.

For a friends-and-family demo where the "aha moment" is endorsement scarcity, user comprehension matters more than model flexibility.

## Decision

### Endorsement slots (not continuous influence)

Each user has a fixed number of **endorsement slots** (k). Each active trust edge (non-revoked endorsement) occupies one slot.

- **Demo value:** k = 3 (tight budget forces deliberate allocation).
- **Production default:** k = 5 (subject to calibration from observed behavior).
- **Display:** "2 of 3 endorsements used" — immediately comprehensible.

When all slots are occupied, the user must revoke an existing endorsement before creating a new one. There is no partial-strength endorsement — a slot is either used or available.

Platform trust level (computed from the user's own graph position) may increase the slot count over time. A well-established user with high diversity and long tenure might earn additional slots. The mechanism for this is not yet defined and will be calibrated from real usage data.

**Platform/verifier accounts are exempt from slot limits.** Accounts designated as platform verifiers (see ADR-008) issue endorsements as infrastructure, not as peer trust signals. These accounts have effectively unlimited endorsement capacity. This exemption is necessary for bootstrapping — the initial verifier must be able to onboard the first cohort of users without exhausting a personal slot budget. Long-term, the system's credibility depends on users graduating from platform-issued endorsements to peer-verified handshakes.

### Renewable daily action budget

Separate from endorsement slots, each user has a daily **action budget** — a renewable resource that limits how many trust actions they can perform per reconciliation cycle (see ADR-021).

- Actions include: endorse, revoke, denounce.
- The budget regenerates each day (use-it-or-lose-it — unused actions do not accumulate).
- Default budget: TBD. Likely 1-3 actions per day for the demo.
- Denouncements may cost more than endorsements (higher friction for adversarial actions).

The daily budget creates a rate limit that is distinct from the capacity limit (slots). You might have 2 empty slots but only 1 remaining action today — you can endorse one person, not two.

### Sponsorship risk (principle, not mechanism)

Sponsors bear risk for the people they endorse. The design intent is:

- Endorsing someone who turns out to be a Sybil or bad actor should have consequences for the sponsor.
- Risk should decrease as the endorsee establishes independent trust (diverse handshakes from others), because the network has independently validated them.
- This creates a natural incentive: don't just invite people, help them integrate into the trust graph.

**The specific mechanism for computing and applying sponsorship risk is not yet decided.** It may involve:
- A penalty to the sponsor's trust distance when an endorsee is denounced.
- Reduction in the sponsor's available slots while their endorsees remain low-diversity.
- A "probation" period for new endorsements that resolves when the endorsee reaches a diversity threshold.

This will be a separate ADR when the model solidifies, likely informed by the red/blue graph simulation work (GitHub issue #624).

### Denouncement budget (separate from endorsement slots)

Users have a small, finite denouncement budget (d = 2). Filing a denouncement consumes one slot permanently (non-refundable). This prevents spam-flagging while allowing genuine concerns to be raised.

Denouncements are recorded but **do not currently affect trust graph traversal**. The future penalty system will be designed separately.

## Consequences

### Positive
- Discrete slots are immediately legible to non-technical users ("you have 3 endorsements").
- Scarcity forces deliberate allocation — users must decide who is worth their limited trust.
- The daily action budget prevents burst attacks (dumping all endorsements at once before a vote).
- Total trust volume in the system is bounded by `N_users * k`, growing linearly with real humans.

### Negative
- The backend must be refactored from continuous influence to discrete slots. The `trust__user_influence` table's `staked_influence` / `spent_influence` model doesn't map cleanly to slot counting.
- Slot count as a function of platform trust is undefined. Until this is designed, k is a static constant.
- Sponsorship risk is a stated principle without an implementation — the incentive structure exists in theory but not in code.

### Neutral
- The existing `influence_staked` column on endorsement records could be repurposed to track "1 slot consumed" as a boolean-ish value, or the column could be removed in favor of a simple count query.
- The daily action quota (currently 5 actions/day enforced by counting `trust__action_queue` rows) is close to the desired design — the main change is making it a first-class concept with its own UI, not a hidden rate limit.

## Alternatives considered

### Continuous influence budget (current implementation)
- Flexible — different actions can cost different amounts of influence.
- Rejected for the demo because it's hard to explain. "You have 6.3 influence remaining" doesn't mean anything to a non-technical user. "You have 1 endorsement slot left" does.
- Revisited in [ADR-023](023-fixed-slot-variable-weight.md): variable-weight endorsements are supported, but each still costs one slot. The continuous budget model remains rejected.

### Unlimited endorsements with diminishing returns
- Each additional endorsement from the same user carries less weight (e.g., weight * 0.8^n).
- Elegant mathematically but invisible to users — they can't see or reason about the diminishing effect.
- Rejected because it prioritizes algorithmic elegance over user comprehension.

### Stack-ranked endorsements (TRD design)
- Users rank their endorsements; only top-k are active.
- Adds complexity (ranking UI, re-ranking on changes) without clear benefit over simple "revoke and re-endorse."
- Rejected for MVP. If users need to manage >5 endorsements with priority, ranking could be revisited.

## References
- [ADR-017: Two-layer trust architecture](017-two-layer-trust-architecture.md) — slots and budgets are platform trust concepts
- [ADR-018: Handshake protocol](018-handshake-protocol.md) — endorsements are the edges that consume slots
- [ADR-019: Trust engine computation](019-trust-engine-computation.md) — scores that may drive dynamic slot allocation
- [ADR-021: Batch reconciliation](021-batch-reconciliation.md) — action budgets regenerate each reconciliation cycle
- [ADR-023: Fixed slots with variable weight](023-fixed-slot-variable-weight.md) — resolves the variable-cost endorsement question
- [ADR-024: Denouncement mechanism](024-denouncement-mechanism.md) — denouncer-only edge revocation as baseline
- [GitHub #624: Trust graph red/blue simulation](https://github.com/icook/tiny-congress/issues/624) — will inform sponsorship risk design
- TRD §4 (Reputation Scarcity Model) — original slot-based design
- `service/src/trust/repo/influence.rs` — current continuous influence implementation
