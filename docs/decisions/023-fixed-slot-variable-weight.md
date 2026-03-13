# ADR-023: Fixed Slots with Variable Weight — No Fractional Budgets

## Status
Draft

## Context

ADR-020 established discrete endorsement slots (k=3 demo, k=5 production) as the scarcity mechanism. It noted that continuous influence "may be revisited if the system needs variable-cost endorsements."

Variable-cost endorsements are now needed. Endorsement weight should reflect two quality dimensions:

- **Swap method:** in-person QR, video chat, text, or email — measuring verification strength.
- **Relationship depth:** how long and how well the endorser knows the endorsee — measuring trust signal quality.

These two knobs produce a weight in (0, 1.0] for each endorsement. The question is: should a low-weight endorsement cost less budget than a high-weight one?

The trust graph simulation framework (PR #643, GitHub #624) provided empirical data on this question. The `sim_multi_point_attachment` scenario demonstrates that a cluster attached at 2+ bridge nodes defeats diversity checks — and fractional budgets increase the number of edges a single node can emit, directly enabling this attack topology.

## Decision

### Keep fixed-cost slots. Vary weight, not budget cost.

Every endorsement costs exactly one slot, regardless of weight. A weight=0.2 email-based endorsement and a weight=1.0 in-person endorsement both consume one slot.

Weight is set at endorsement creation based on the swap method and self-reported relationship depth. The trust engine uses weight to compute edge cost (cost = 1/weight), so low-weight endorsements naturally contribute less to the recipient's trust score without any budget system changes.

### Weight transparency in the endorsement flow

The UX problem is not "I wasted a slot" — it's "I didn't know this endorsement would be weak." The endorsement UI must show the user the resulting weight before they commit the slot:

- Display the weight impact clearly: "In-person + known for years = strong endorsement" vs "Email + just met = weak endorsement"
- Let the user make an informed decision about whether a weak endorsement is worth a slot
- This makes the hoarding dynamic a feature: users think carefully before endorsing

### Endorsement weight parameters

Weight is computed from two inputs at endorsement time:

| Swap method | Base weight |
|---|---|
| In-person QR | 1.0 |
| Video chat | 0.7 |
| Text/messaging | 0.4 |
| Email | 0.2 |

| Relationship depth | Multiplier |
|---|---|
| Years (deep trust) | 1.0 |
| Months (moderate) | 0.7 |
| Acquaintance | 0.5 |

Final weight = base × multiplier, clamped to the DB constraint range (0, 1.0].

These values are initial estimates. The simulation framework can be used to calibrate them against adversarial topologies before launch.

## Consequences

### Positive
- **Sybil resistance preserved.** Total edges per node remain bounded by k. A bot farm with k=3 gets exactly 3 edges regardless of weight, keeping graph density calculable.
- **Hoarding is the correct incentive.** Users saving slots for high-quality endorsements produces a sparse, high-signal trust graph. n=3 fanout covers the globe in ~20 rounds — the network doesn't need every user spending all slots to grow.
- **No schema changes.** The `weight` column on `reputation__endorsements` already exists with `CHECK (weight > 0 AND weight <= 1.0)`. Slot counting is unchanged.
- **Testable.** The simulation framework can evaluate weight distributions against adversarial topologies before choosing final values.

### Negative
- **Low-weight endorsements feel expensive.** Spending 1 of 3 slots on a weight=0.2 endorsement may feel like a bad deal. Mitigated by weight transparency — user sees the tradeoff before committing.
- **Weight values need calibration.** The base × multiplier table is an initial estimate. Bad values could make some tiers useless (too weak to help) or others overpowered. The simulation framework is the calibration tool.
- **Self-reported relationship depth is gameable.** A Sybil operator will always claim "deep trust." Mitigated by the fact that the slot cost is fixed — even at max self-reported depth, they still only get k edges.

### Neutral
- ADR-020's mention of revisiting continuous influence for variable-cost endorsements is resolved: the answer is no. ADR-020 should link to ADR-023 when this ADR is accepted.
- The endorsement flow needs UI work to surface weight, but this aligns with the demo goal of making endorsement feel deliberate and meaningful.

## Alternatives considered

### Fractional budgets (rejected)

Replace integer slots with a continuous budget (e.g., 2.0). Low-weight endorsements cost less budget (e.g., 0.2), allowing more endorsements.

Rejected because:
- **Sybil density explosion.** A budget of 2.0 at 0.2 cost per endorsement = 10 edges per node, vs 3 with fixed slots. The `sim_multi_point_attachment` scenario demonstrates that multi-bridge attachment defeats diversity checks — more edges directly enables this.
- **UX complexity.** "You have 1.4 budget remaining" is harder to reason about than "1 of 3 slots used."
- **Wrong incentive.** Encourages users to spray weak endorsements instead of making deliberate, high-quality trust signals.

### Weight-proportional slot cost (rejected)

Each endorsement costs `ceil(weight * 2)` slots — a weight=0.2 endorsement costs 1 slot, weight=1.0 costs 2.

Rejected because it inverts the incentive: strong endorsements become more expensive, encouraging users to prefer weak ones. The system should reward conviction, not penalize it.

### No variable weight (status quo)

Keep all endorsements at weight=1.0 regardless of verification method.

Rejected because it treats an in-person handshake identically to an email introduction, which doesn't reflect actual trust signal quality. The trust engine's distance computation is only meaningful if edge weights carry real information.

## References
- [ADR-020: Reputation scarcity](020-reputation-scarcity.md) — establishes slot model, notes continuous influence may be revisited
- [ADR-019: Trust engine computation](019-trust-engine-computation.md) — weight → cost → distance computation
- [GitHub #624: Trust graph simulation](https://github.com/icook/tiny-congress/issues/624) — empirical data on multi-point attachment attacks
- PR #643: Simulation harness including `sim_multi_point_attachment` scenario
- Migration 12: `CHECK (weight > 0 AND weight <= 1.0)` constraint on endorsements
