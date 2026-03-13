# Open Questions: Trust Simulation & Denouncement Mechanisms

**Date:** 2026-03-13
**Branch:** test/624-trust-simulation-harness (#643)
**Context:** Consolidated from simulation harness build, adversarial audit, and mechanism comparison sessions. Updated with mechanism decisions from review conversation.

---

## Current state (2026-03-13)

**Where things stand:** The simulation harness is built and tested (shipped in PR #663). GraphSpec extraction and proptest/temporal extensions are on PR #673 (pending CI). Three denouncement mechanisms have been simulated; two are rejected, one is the chosen baseline. Three draft ADRs need simulation evidence to accept.

**Active branches/PRs:**
- **PR #643** (`test/624-trust-simulation-harness`) — this `.plan/` design workspace. Reference only, not meant to merge.
- **PR #673** (`feature/662-graphspec-extraction`) — GraphSpec extraction, behavioral predicates, proptest integration, temporal extensions. Must merge before Phase 1 can start.
- **`sim/open-questions-workspace`** — latest updates to this doc and the phased plan.

**Key decisions already made:**
- Nuclear edge removal: REJECTED (weaponizable)
- Score penalty: REJECTED (stacks linearly, weaponizable)
- Denouncer-only revocation: CHOSEN as baseline mechanism (ADR-024 draft)
- Loss function: bias defensive — false negatives >> false positives in cost. Blue casualties from cascade acceptable. Asymmetry narrows at scale.
- Renewal mechanism: re-swap (no new UX needed)
- Denouncement propagation = sponsorship cascade (same mechanism, not separate concepts)

**What's next:** Phase 1 of the simulation plan — validate denouncer-only revocation + cascade against adversarial topologies to accept ADR-024. See `.plan/2026-03-13-simulation-phases-plan.md` for the full 14-step plan.

**Open question scoreboard:** 19 questions total. 7 resolved (Q1-2 mechanism rejection, Q7 loss function, Q9 cross-ref, Q13 renewal, Q17 cascade=propagation, plus ADR-023 accepted). 10 answerable by simulation (Q4-6, Q8, Q12, Q14-16, Q18-19). 2 deferred for design (Q3 adjudication, #656 weight UI).

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

7. ~~**Loss function for tuning.**~~ **RESOLVED (directionally).** Bias defensive: false negatives (bad actors passing) are much more costly than false positives (legitimate users temporarily downgraded). Rationale: early in the network, real-world consequences of being downgraded are minimal, and the remediation path is organic (seek a fresh endorsement from someone who actually knows you). Blue casualties from cascade collateral (e.g., the 1/7 in the mercenary scenario) are acceptable. This asymmetry should narrow as the network matures and trust status carries real weight — the threshold shifts toward due process at scale. For simulation: `W_block >> W_collateral` in the loss function; accept scenarios where cascade causes blue casualties if it also blocks the red target.
8. **Propagation penalty values.** Currently hardcoded at 2.0 distance / 1 diversity (lighter than the primary 3.0/1). Should these be tuned independently, or always be a fixed fraction of the primary penalty? This is the tuning knob for Q16-19 (denouncement propagation) — the cascade mechanism already exists in `apply_sponsorship_cascade`; the open question is whether the penalty values are right.

---

## Time decay

Trust edges should decay over time — this naturally models real human interaction. Relationships that aren't renewed become stale.

### Open questions

12. **Decay model.** What function describes decay? Linear, exponential, step-function (e.g., weight halves after 1 year without interaction)? The choice affects how aggressively the graph prunes inactive relationships.
13. ~~**Renewal mechanism.**~~ **RESOLVED.** Users re-do the handshake (re-swap) to renew. This resets the decay clock and can upgrade the weight (e.g., text→QR as trust deepens). No new UX needed — the existing swap flow handles it. A re-swap overwrites the existing slot with new weight + fresh timestamp.
14. **Interaction with slot budget.** A decayed endorsement still occupies a slot. Does the user need to explicitly revoke it to free the slot, or does it auto-release below some weight threshold? Auto-release is simpler for users but means the graph topology changes without explicit action.
15. **Simulation coverage.** The current harness has no time dimension. Need to add a temporal axis to topologies (edge age) and measure how decay affects adversarial scenarios — e.g., does a Sybil cluster's attack window narrow naturally as fabricated edges decay?

---

## Denouncement propagation

Endorsing someone who later gets denounced should carry consequences. This is "part of the risk of endorsement" — you stake your reputation when you vouch for someone.

### Open questions

16. **Propagation model.** How far does the consequence travel? Options: one hop (direct endorsers only), attenuated multi-hop (penalty decreases with distance from the denounced), or full cascade to the anchor. One-hop is simplest and most predictable.
17. ~~**Relationship to sponsorship cascade.**~~ **RESOLVED.** Denouncement propagation IS the sponsorship cascade — same mechanism, different framing. `apply_sponsorship_cascade` already implements one-hop propagation (endorsers of the target are penalized). Q8's penalty values are the tuning knobs for this mechanism. The remaining design questions (Q16, Q18, Q19) refine propagation depth and proportionality.
18. **Proportionality.** If Alice endorses Bob and Bob gets denounced by Charlie, how much should Alice's score suffer? The current cascade uses a fixed 2.0 distance / 1 diversity penalty. Should this scale with: how many people denounced Bob? How strong Alice's endorsement of Bob was? How long ago Alice endorsed Bob (interaction with time decay)?
19. **Circular denouncement risk.** If propagation is automatic, can a denouncement cascade loop? A→B→C→A could create runaway penalty accumulation. Need to either prove this is impossible in the graph structure or add visited-set protection.

---

## Architectural questions

9. **ADR-020 cross-reference.** ADR-020 says continuous influence "may be revisited for variable-cost endorsements." ADR-023 resolves this question (answer: no). ADR-020 should link to ADR-023 when it's accepted.
10. **Denouncement budget interaction.** ADR-020 sets d=2 denouncement budget. With denouncer-only revocation as the baseline mechanism, the budget question simplifies: each denouncement costs 1 budget and revokes your edge to the target. The adjudication path (severe slashing) is a separate governance action, not a budget spend.
11. **Engine runs twice per measurement.** `SimulationReport::run()` computes scores in memory, then `materialize()` calls `recompute_from_anchor` which re-runs the engine and writes to snapshots. Safe in tests, but 2x engine cost per measurement. Worth fixing if the simulation suite grows significantly.

---

## Next actions (roughly prioritized)

### Done
- [x] **Mechanism recommendation** — denouncer-only revocation as baseline; nuclear edge removal and score penalty rejected; adjudication for severe cases is future work
- [x] **ADR-020 ↔ ADR-023 cross-reference** (question 9) — done
- [x] **Finalize ADR-023** — accepted; weight table values are provisional, structural decision is final
- [x] **Q13 resolved** — renewal = re-do handshake (re-swap overwrites slot with new weight + fresh timestamp)
- [x] **Q17 resolved** — denouncement propagation IS the sponsorship cascade, same mechanism

### Phase 1: Validate denouncer-only revocation → accept ADR-024
- [ ] Add `apply_denouncer_revocation(denouncer, target)` to mechanisms.rs
- [ ] Re-run comparison with all 4 mechanisms across existing adversarial scenarios
- [ ] New scenario: coordinated denouncement (3 independent denouncers vs. well-connected target)
- [ ] New scenario: insufficient denouncement (single denouncer vs. well-connected target, confirm survival)
- [ ] **Simulate propagation** (Q8, Q16, Q18, Q19) — run `apply_sponsorship_cascade` alongside denouncer-only revocation; sweep penalty values (Q8); test one-hop vs. multi-hop (Q16); test proportionality scaling (Q18); verify no circular cascades (Q19)
- [ ] Accept ADR-024 with simulation evidence

### Phase 2: Weight variance → stress-test ADR-023
- [ ] Mixed-weight adversarial scenarios using ADR-023 table values (Q4)
- [ ] Weight sweep on mercenary-bot scenario (Q5 calibration criteria)
- [ ] Verify Sybil at max-weight still fails diversity checks (Q6)

### Phase 3: Time decay experiments → accept ADR-025
- [ ] Compare 3 decay functions: exponential, step, linear (Q12)
- [ ] Temporal adversarial scenarios: Sybil attack window narrowing under decay (Q15)
- [ ] Stale-but-legitimate edges: do real relationships survive without renewal?
- [ ] Slot auto-release policy: what happens to decayed slots? (Q14)

### Deferred (needs design, not simulation)
- [ ] **Multi-method weight UI** — #656: add swap method + relationship depth selection to endorsement flow
- [ ] **Adjudication process design** (Q3) — governance process for severe slashing; its own ADR
