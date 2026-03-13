# Open Questions: Trust Simulation & Denouncement Mechanisms

**Date:** 2026-03-13
**Branch:** test/624-trust-simulation-harness (#643)
**Context:** Consolidated from simulation harness build, adversarial audit, and mechanism comparison sessions. Updated with mechanism decisions from review conversation.

---

## Mechanism selection

The comparison framework produced initial results. Several mechanisms have been ruled out; the design direction is converging on denouncer-only revocation + adjudicated slashing for severe cases.

### What the data shows

| Mechanism | Removes bad actors? | Weaponization-resistant? | Collateral damage |
|---|---|---|---|
| Edge removal (nuclear) | Yes (target becomes unreachable) | YES — only affects target's edges | None in tested scenarios |
| Score penalty | Yes (distance/diversity degraded) | NO — stacks to overwhelm legitimate users | None in tested scenarios |
| Sponsorship cascade | Partially (endorsers penalized, edges revoked) | YES — penalties hit endorsers, not target directly | 1/7 blue nodes in mercenary scenario |

### Decisions made

1. **Nuclear edge removal is non-viable.** ~~REJECTED.~~ One denouncement severs all inbound edges — too easily weaponized. A single malicious actor can completely disconnect a legitimate user.
2. **Score penalty is non-viable.** ~~REJECTED~~ (from simulation data). Stacks linearly and is trivially weaponizable by coordinated groups.
3. **Denouncer-only edge revocation is the baseline mechanism.** When you denounce someone, your endorsement edge to them is revoked. You can't simultaneously endorse and denounce. This is the proportionate, obvious response — "I no longer vouch for this person." It's soft enough that a single bad-faith denouncement only costs the target one path, not all of them.
4. **Threshold cascade becomes an adjudication problem.** The "right" approach for severe action (full disconnection, slashing) is not an automated threshold but a governance process: a motion is raised to slash, evidence is brought, and broad consensus from a diverse, deeply trusted quorum is solicited. The threshold should be set very conservatively — essentially "the graph is in consensus." This is future work beyond the simulation harness.

### Remaining questions

2. ~~Can score penalty be made weaponization-resistant?~~ **Deprioritized.** Mechanism rejected. A cap or diminishing returns curve could be revisited but the fundamental stacking problem makes this less attractive than denouncer-only revocation.
3. **Adjudication design.** How does the governance process for severe slashing work? Who can raise a motion? What quorum is required? What evidence format? This is a substantial design problem — likely its own ADR.

---

## Weight calibration

ADR-023 proposes a weight table (swap method x relationship depth) but the values are initial estimates.

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

## Time decay

Trust edges should decay over time — this naturally models real human interaction. Relationships that aren't renewed become stale.

### Open questions

12. **Decay model.** What function describes decay? Linear, exponential, step-function (e.g., weight halves after 1 year without interaction)? The choice affects how aggressively the graph prunes inactive relationships.
13. **Renewal mechanism.** How does a user "renew" an endorsement to reset the decay clock? Re-performing the swap? A lightweight confirmation ("I still vouch for this person")? The UX cost of renewal determines how many edges survive long-term.
14. **Interaction with slot budget.** A decayed endorsement still occupies a slot. Does the user need to explicitly revoke it to free the slot, or does it auto-release below some weight threshold? Auto-release is simpler for users but means the graph topology changes without explicit action.
15. **Simulation coverage.** The current harness has no time dimension. Need to add a temporal axis to topologies (edge age) and measure how decay affects adversarial scenarios — e.g., does a Sybil cluster's attack window narrow naturally as fabricated edges decay?

---

## Denouncement propagation

Endorsing someone who later gets denounced should carry consequences. This is "part of the risk of endorsement" — you stake your reputation when you vouch for someone.

### Open questions

16. **Propagation model.** How far does the consequence travel? Options: one hop (direct endorsers only), attenuated multi-hop (penalty decreases with distance from the denounced), or full cascade to the anchor. One-hop is simplest and most predictable.
17. **Relationship to sponsorship cascade.** The existing `apply_sponsorship_cascade` mechanism already penalizes endorsers of the target. Denouncement propagation generalizes this — it's not just a mechanism applied by an admin, it's automatic. Should the simulation's sponsorship cascade evolve into the propagation model, or are they separate concepts?
18. **Proportionality.** If Alice endorses Bob and Bob gets denounced by Charlie, how much should Alice's score suffer? The current cascade uses a fixed 2.0 distance / 1 diversity penalty. Should this scale with: how many people denounced Bob? How strong Alice's endorsement of Bob was? How long ago Alice endorsed Bob (interaction with time decay)?
19. **Circular denouncement risk.** If propagation is automatic, can a denouncement cascade loop? A→B→C→A could create runaway penalty accumulation. Need to either prove this is impossible in the graph structure or add visited-set protection.

---

## Architectural questions

9. **ADR-020 cross-reference.** ADR-020 says continuous influence "may be revisited for variable-cost endorsements." ADR-023 resolves this question (answer: no). ADR-020 should link to ADR-023 when it's accepted.
10. **Denouncement budget interaction.** ADR-020 sets d=2 denouncement budget. With denouncer-only revocation as the baseline mechanism, the budget question simplifies: each denouncement costs 1 budget and revokes your edge to the target. The adjudication path (severe slashing) is a separate governance action, not a budget spend.
11. **Engine runs twice per measurement.** `SimulationReport::run()` computes scores in memory, then `materialize()` calls `recompute_from_anchor` which re-runs the engine and writes to snapshots. Safe in tests, but 2x engine cost per measurement. Worth fixing if the simulation suite grows significantly.

---

## Next actions (roughly prioritized)

- [x] **Mechanism recommendation** — denouncer-only revocation as baseline; nuclear edge removal and score penalty rejected; adjudication for severe cases is future work
- [ ] **Simulate denouncer-only revocation** — add `apply_denouncer_revocation(denouncer, target)` to mechanisms.rs, re-run comparison to verify it's effective enough against adversarial topologies
- [ ] **Time decay in simulation** (questions 12–15) — add temporal axis to simulation harness; produce dev-targeted doc for UI rendering covering decay model, renewal UX, and edge cases. See ADR-025.
- [ ] **Denouncement propagation design** (questions 16–19) — decide propagation depth, relationship to cascade, proportionality rules
- [ ] **Add weight variance scenarios** (question 4) — mixed-weight topologies in the comparison
- [ ] **Loss function conversation** (question 7) — needed before automated tuning
- [ ] **Multi-method weight UI** — #656: add swap method + relationship depth selection to endorsement flow
- [x] **ADR-020 ↔ ADR-023 cross-reference** (question 9) — done
- [x] **Finalize ADR-023** — accepted; weight table values are provisional, structural decision is final
- [ ] **Adjudication process design** (question 3) — governance process for severe slashing; likely its own ADR
