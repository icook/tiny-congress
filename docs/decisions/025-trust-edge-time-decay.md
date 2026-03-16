# ADR-025: Trust Edge Time Decay

## Status
Accepted (2026-03-13)

## Context

Trust edges in the graph currently have no temporal dimension — an endorsement created two years ago with no further interaction carries the same weight as one created yesterday. This doesn't reflect how trust works in practice: relationships that aren't maintained become stale, and the trust signal they carry degrades.

Time decay serves multiple purposes in the trust model:

- **Accuracy.** A two-year-old endorsement from someone you've lost touch with is a weaker signal than a recent one from an active relationship.
- **Natural Sybil resistance.** Fabricated edges created by a Sybil operator decay over time, narrowing the attack window. The attacker must continuously re-create edges to maintain graph position, increasing operational cost.
- **Graph hygiene.** Without decay, the graph accumulates stale edges indefinitely. Decay naturally prunes inactive relationships without requiring explicit revocation.

The trust graph simulation framework (PR #643) can model decay once a temporal axis is added to test topologies. The decay model should be designed within the simulation harness, validated against adversarial scenarios, and documented for frontend developers to implement the renewal UX.

## Decision

### Trust edges will decay over time. The specific model is TBD pending simulation.

The commitment is that time decay is a required property of the trust graph, not an optional enhancement. The simulation harness will be used to evaluate candidate decay functions and their interaction with adversarial topologies before selecting the final model.

### Open design questions

These were resolved by simulation experiments (PR #679):

1. **Decay function.** Step function: weight is 1.0 at creation, drops to 0.5 at 1 year without renewal, and drops to 0.0 at 2 years. This provides a clear, predictable degradation timeline.
2. **Renewal mechanism.** Re-swap (re-performing the handshake resets the decay clock and can upgrade the weight, e.g., text→QR as trust deepens). No new UX needed — the existing swap flow handles it.
3. **Slot interaction.** Auto-release below weight 0.05 (fully decayed edges release the slot). This simplifies the user experience by not requiring explicit revocation of dead edges.
4. **Engine integration.** Batch reconciliation (ADR-021 reconciliation cycle). Simpler than query-time decay and aligns with existing reconciliation infrastructure.

### Simulation deliverable

The simulation harness should produce a developer-targeted document covering:
- Recommended decay function with parameters
- Interaction with adversarial topologies (does decay narrow Sybil attack windows?)
- Renewal UX requirements for frontend implementation
- Edge cases (what happens to a user whose only inbound edges all decay?)

## Consequences

### Positive
- **Passive Sybil resistance.** Attack edges decay without any user action, raising the cost of maintaining a Sybil cluster.
- **Graph accuracy.** Edge weights reflect current relationship state, not historical.
- **Composable with denouncement.** Decay + denouncer-only revocation (ADR-024) means bad actors face both active removal (denouncement) and passive erosion (decay).

### Negative
- **Renewal burden.** Users must periodically confirm endorsements or lose them. If renewal is too costly, legitimate edges decay and the graph becomes sparse.
- **Complexity.** Adds a temporal dimension to every trust computation. Batch decay was chosen (ADR-021 reconciliation cycle) over query-time decay.
- **Edge cases.** A user whose endorsers all go inactive could lose trust standing through no fault of their own.

### Neutral
- Interacts with ADR-023's weight model: the step function applies a decay multiplier to the base weight (swap method × relationship depth). At 1 year the effective weight is 0.5× the base; at 2 years the edge is removed entirely.
- Interacts with ADR-024's denouncement cascade: if Alice's endorsement of Bob has decayed below 0.05 (auto-released), she no longer holds an edge to Bob and would not be penalized when Bob is denounced.

## References
- [ADR-024: Denouncement mechanism](024-denouncement-mechanism.md) — denouncer-only revocation as baseline
- [ADR-023: Fixed slots with variable weight](023-fixed-slot-variable-weight.md) — weight model that decay modifies
- [ADR-021: Batch reconciliation](021-batch-reconciliation.md) — potential execution model for periodic decay
- [GitHub #624: Trust graph simulation](https://github.com/icook/tiny-congress/issues/624) — simulation harness for validating decay models
