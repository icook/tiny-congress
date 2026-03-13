# Open Questions: Trust Simulation & Denouncement Mechanisms

**Date:** 2026-03-13
**Branch:** test/624-trust-simulation-harness (#643)
**Context:** Consolidated from simulation harness build, adversarial audit, and mechanism comparison sessions.

---

## Mechanism selection

The comparison framework produced initial results. No mechanism is clearly dominant.

### What the data shows

| Mechanism | Removes bad actors? | Weaponization-resistant? | Collateral damage |
|---|---|---|---|
| Edge removal | Yes (target becomes unreachable) | YES — only affects target's edges | None in tested scenarios |
| Score penalty | Yes (distance/diversity degraded) | NO — stacks to overwhelm legitimate users | None in tested scenarios |
| Sponsorship cascade | Partially (endorsers penalized, edges revoked) | YES — penalties hit endorsers, not target directly | 1/7 blue nodes in mercenary scenario |

### Open questions

1. **Is edge removal too aggressive?** It severs all inbound edges — the target is completely disconnected. Is there a softer version (revoke only the denouncer's edge to the target) that still works?
2. **Can score penalty be made weaponization-resistant?** The current model stacks linearly (10 denouncements = 30.0 distance penalty). A cap or diminishing returns curve would resist mass-denouncement but might also weaken the mechanism against real bad actors.
3. **Should we test a threshold cascade?** Require >= 2 independent denouncements before cascade fires. This directly addresses the weaponization problem — a single Sybil cluster's denouncements wouldn't meet the independence threshold. Highest-value hybrid to test next.

---

## Weight calibration

ADR-022 proposes a weight table (swap method x relationship depth) but the values are initial estimates.

### Open questions

4. **Weight variance simulation.** Current test topologies use uniform weights (mostly 1.0). The mechanism ranking might change with realistic weight distributions. Need to add scenarios with mixed weights (e.g., some endorsements at 0.3, others at 1.0) and re-run the comparison.
5. **Calibrating the weight table.** The simulation framework can sweep weight parameters against adversarial topologies. What are the acceptance criteria? Proposed: "all 6 baseline adversarial scenarios still produce the expected outcome (red blocked, blue passes) across the weight range."
6. **Self-reported relationship depth is gameable.** A Sybil operator will always claim "deep trust." The DB weight cap (CHECK <= 1.0) bounds the damage, and fixed slot cost means they still only get k edges. Is this sufficient, or do we need server-side validation of relationship claims?

---

## Parameter tuning

The comparison framework supports sweeping penalty values programmatically.

### Open questions

7. **Loss function for tuning.** To automate parameter selection, we need `score = f(targets_removed, blue_casualties, weaponization_survived)`. What's the relative weight of false positives vs false negatives? This is a values question, not an engineering question — different communities might want different tradeoffs.
8. **Penalty values for sponsorship cascade.** Currently hardcoded at 2.0 distance / 1 diversity (lighter than the primary 3.0/1). Should these be tuned independently, or always be a fixed fraction of the primary penalty?

---

## Architectural questions

9. **ADR-020 cross-reference.** ADR-020 says continuous influence "may be revisited for variable-cost endorsements." ADR-022 resolves this question (answer: no). ADR-020 should link to ADR-022 when it's accepted.
10. **Denouncement budget interaction.** ADR-020 sets d=2 denouncement budget. If we choose threshold cascade (question 3), the threshold interacts with the budget — with d=2 and threshold=2, a single user can't trigger a cascade alone, which may be desirable. Need to model this interaction.
11. **Engine runs twice per measurement.** `SimulationReport::run()` computes scores in memory, then `materialize()` calls `recompute_from_anchor` which re-runs the engine and writes to snapshots. Safe in tests, but 2x engine cost per measurement. Worth fixing if the simulation suite grows significantly.

---

## Next actions (roughly prioritized)

- [ ] **Review comparison output** and make a preliminary mechanism recommendation based on current data
- [ ] **Test threshold cascade hybrid** (question 3) — add `apply_threshold_cascade` to mechanisms.rs
- [ ] **Add weight variance scenarios** (question 4) — mixed-weight topologies in the comparison
- [ ] **Loss function conversation** (question 7) — needed before automated tuning
- [ ] **ADR-020 ↔ ADR-022 cross-reference** (question 9) — quick edit once ADR-022 is accepted
- [ ] **Finalize ADR-022** — currently Draft, needs review of weight table values
