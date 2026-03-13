# Open Questions: Trust Simulation & Denouncement Mechanisms

**Date:** 2026-03-13
**Branch:** test/624-trust-simulation-harness (#643)
**Context:** Consolidated from simulation harness build, adversarial audit, mechanism comparison, and scale simulation sessions.

---

## Current state (2026-03-13, updated)

**Where things stand:** All four trust ADRs are accepted with simulation evidence. The mechanism design phase is complete. Scale simulation (PR #684) has validated the system to ~5k users with high confidence. The remaining work is engineering (sparse max-flow migration) and topology realism (community-structure testing).

**Key insight: getting to 100k is an engineering problem, not a design problem.** The trust mechanisms (distance, diversity, denouncement, decay) are scale-invariant — the math doesn't change with graph size. What breaks at scale is the engine implementation (dense O(n²) max-flow matrix) and our confidence in topology assumptions (BA graphs are unrealistically well-connected). See `.plan/2026-03-13-scale-analysis-findings.md` for full analysis.

**Active branches/PRs:**
- **PR #676** (`sim/trust-simulation-design-workspace`) — this `.plan/` design workspace. Reference only, not meant to merge.
- **PR #678** — adversarial simulation suite (Phases 1-2). ADR-024 accepted with 31 tests.
- **PR #679** — time decay simulation (Phase 3). ADR-025 accepted.
- **PR #684** (`test/680-scale-simulation-framework`) — scale simulation: BA graph generation, Sybil mesh analysis, sparse max-flow, 8 scale test scenarios.

**All trust ADRs accepted:**
- ADR-020: Endorsement Slots & Denouncement Budget (k=10, d=2)
- ADR-023: Fixed Slots with Variable Weight (weight table stress-tested in Phase 2, PR #678)
- ADR-024: Denouncer-Only Edge Revocation (accepted 2026-03-13, 31 tests in PR #678)
- ADR-025: Trust Edge Time Decay (step function: 1.0 yr1, 0.5 yr2, 0.0 after; accepted 2026-03-13, PR #679)

**Key decisions made (mechanism phase):**
- Nuclear edge removal: REJECTED (weaponizable)
- Score penalty: REJECTED (stacks linearly, weaponizable)
- Denouncer-only revocation: ACCEPTED (ADR-024)
- Cascade complement: 2.0/1 penalty, one-hop only
- Loss function: bias defensive — false negatives >> false positives
- Renewal mechanism: re-swap (no new UX needed)
- Denouncement propagation = sponsorship cascade (same mechanism)
- Penalty operating point: 2.0 distance / -1 diversity (confirmed by sweep)
- Time decay: step function (1.0/0.5/0.0 at 1yr/2yr thresholds)

**Scale confidence assessment:**

| Scale | Confidence | Key constraint |
|---|---|---|
| 1k–5k | **High** | Mechanisms are scale-invariant. BA simulations confirm robust connectivity. |
| 5k–10k | **Medium** | Engine FlowGraph hits memory wall (O(n²) dense matrix). Sparse implementation proven in tests. |
| 10k–100k | **Low-Medium** | Mechanism math is sound. Engine perf, realistic topology, and sophisticated Sybil strategies untested. |

**Open question scoreboard:** 23 questions total. 16 resolved through simulation + ADR acceptance. 4 new scale questions (Q20-Q23). 3 deferred for design/engineering.

**Scale readiness:** See `.plan/2026-03-13-scale-readiness-matrix.md` for the tier-by-tier gate criteria and evidence requirements. Tiers 0-1 PASS. Tier 2 BLOCKED on #680, #681, #682.

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

## Scale simulation findings (PR #684)

See `.plan/2026-03-13-scale-analysis-findings.md` for full analysis. Summary:

1. **Sybil mesh diversity = bridge count, exactly.** Internal mesh endorsements don't inflate diversity. Security reduces to "how hard is it to compromise 2+ independent endorsers?"
2. **Engine FlowGraph is the bottleneck.** Dense O(n²) matrix: 4MB at 1k, 100MB at 5k, 40GB at 100k. Sparse Edmonds-Karp (O(E)) proven identical in tests — needs to be ported to engine.
3. **BA graphs at 1k-2k:** 100% reachable, mean distance 2.5, min diversity 3. Distance threshold 5.0 is generous.
4. **10k validated (bonus run):** mean distance 2.958, max 5.0, min diversity 3 (sampled 1000), 100% reachable. ~993 seconds.
5. **Bridge removal resilient:** removing 3 highest-degree nodes → 99.7% still reachable.
6. **Correlated decay localized:** 100-node cohort with 2yr+ edges → 0 unreachable, 3 with increased distance. **Caveat:** BA topology flatters the system — real communities cluster harder.

### Open questions (scale)

20. **Real topology modeling.** BA produces unrealistically high connectivity. Need community-structure generators (stochastic block model: dense intra-community, sparse inter-community) to test whether thresholds hold for realistic social graphs. **Ticket: #680.**
21. **Engine sparse max-flow migration.** When to migrate? Current dense impl works at demo scale (~100). Sparse impl proven correct. Could port incrementally. **Ticket: #681.**
22. **Sybil mesh countermeasures.** With diversity=bridge_count proven, what additional detection beyond the diversity threshold? Options: temporal analysis (simultaneous endorsements), graph structure (dense cluster with few external connections), behavioral signals. **Ticket: #682.**
23. **Community-structure testing.** Stochastic block model graphs to re-run all scale tests. Would surface whether current thresholds need adjustment for realistic social structure. Part of #680.

---

## Next actions (roughly prioritized)

### Done (mechanism phases)
- [x] **Phase 1: ADR-024 accepted** — denouncer-only revocation validated with 31 simulation scenarios (PR #678)
- [x] **Phase 2: ADR-023 stress-tested** — weight variance scenarios, max-weight Sybil still fails diversity (PR #678)
- [x] **Phase 3: ADR-025 accepted** — step function decay (1.0/0.5/0.0), temporal adversarial scenarios (PR #679)
- [x] **All 4 trust ADRs accepted** — mechanism design phase complete

### Done (scale simulation)
- [x] **Scale simulation framework** — BA graph generation, sparse max-flow, Sybil mesh analysis (PR #684)
- [x] **Scale confidence assessment** — high to 5k, medium to 10k, low-medium to 100k

### Active (scale hardening)
- [ ] **Community-structure topology testing** (#680) — stochastic block model graphs, re-run scale tests
- [ ] **Correlated failure scenarios** (#682) — realistic cohort clustering, not just BA redundancy
- [ ] **Engine sparse max-flow migration** (#681) — port sparse Edmonds-Karp to `service/src/trust/max_flow.rs`

### Deferred (needs design, not simulation)
- [ ] **Multi-method weight UI** — #656: add swap method + relationship depth selection to endorsement flow
- [ ] **Adjudication process design** (Q3) — governance process for severe slashing; its own ADR
- [ ] **Sybil mesh countermeasures** (#682) — temporal/structural detection beyond diversity threshold
