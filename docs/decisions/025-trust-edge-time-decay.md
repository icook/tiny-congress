# ADR-025: Trust Edge Time Decay

## Status
Draft

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

These will be resolved by simulation experiments and documented in an update to this ADR:

1. **Decay function.** Linear, exponential, or step-function (e.g., weight halves after 1 year without interaction). The choice affects how aggressively the graph prunes.
2. **Renewal mechanism.** How a user resets the decay clock — re-performing the swap, a lightweight confirmation ("I still vouch for this person"), or automatic renewal on interaction. The UX cost of renewal determines how many edges survive long-term.
3. **Slot interaction.** A fully decayed endorsement still occupies a slot. Options: require explicit revocation to free the slot, or auto-release below a weight floor. Auto-release is simpler for users but changes topology without explicit action.
4. **Engine integration.** Decay could be applied at query time (effective_weight = weight × decay_factor(age)) or via periodic batch reconciliation (ADR-021). Query-time is more accurate; batch is simpler and aligns with existing reconciliation infrastructure.

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
- **Complexity.** Adds a temporal dimension to every trust computation. Must decide between query-time decay (accurate, slower) and batch decay (simpler, stale between reconciliation runs).
- **Edge cases.** A user whose endorsers all go inactive could lose trust standing through no fault of their own.

### Neutral
- Interacts with ADR-023's weight model: decay modifies effective weight over time, adding a third dimension beyond swap method and relationship depth.
- May influence denouncement propagation design: if Alice's endorsement of Bob has mostly decayed, should she still be penalized when Bob is denounced?

## References
- [ADR-024: Denouncement mechanism](024-denouncement-mechanism.md) — denouncer-only revocation as baseline
- [ADR-023: Fixed slots with variable weight](023-fixed-slot-variable-weight.md) — weight model that decay modifies
- [ADR-021: Batch reconciliation](021-batch-reconciliation.md) — potential execution model for periodic decay
- [GitHub #624: Trust graph simulation](https://github.com/icook/tiny-congress/issues/624) — simulation harness for validating decay models
