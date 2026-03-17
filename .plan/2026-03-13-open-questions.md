# Open Questions: Trust Simulation & Denouncement Mechanisms

**Date:** 2026-03-13
**Branch:** test/624-trust-simulation-harness (#643)
**Context:** Consolidated from simulation harness build, adversarial audit, mechanism comparison, and scale simulation sessions.

---

## Current state (2026-03-13, updated)

**Where things stand:** All four trust ADRs are accepted with simulation evidence. The mechanism design phase is complete. Scale simulation (PR #684) has validated the system to ~5k users with high confidence.

**Key distinction: mechanism security vs operational security.** The trust mechanisms (distance, diversity, denouncement, decay) are scale-invariant — the math doesn't change with graph size. This part is done and provable. What changes with scale is the operational challenge: engine performance (dense O(n²) max-flow matrix), topology realism (BA graphs flatter the system), and the arms race with attackers who adapt to whatever we build. Getting to 5k is engineering. Getting to 100k is engineering + ongoing operations. "Provably robust at 100k" is not achievable — for this system or any adversarial system at that scale. The realistic goal is bounded confidence with detection and response capability. See `.plan/2026-03-13-scale-readiness-matrix.md` for the tiered framing and `.plan/2026-03-13-scale-analysis-findings.md` for the data.

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

| Scale | Confidence | Nature of work | Key constraint |
|---|---|---|---|
| 1k–5k | **High** | Build and verify (completable) | Mechanisms are scale-invariant. BA simulations confirm robust connectivity. |
| 5k–10k | **Medium** | Build, verify, instrument (completable) | Engine FlowGraph hits memory wall (O(n²) dense matrix). Sparse implementation proven in tests. Need monitoring. |
| 10k–100k | **Low-Medium** | Ongoing operations (never done) | Mechanism math is sound. Engine perf, realistic topology, sophisticated Sybil strategies untested. Requires active detection, response, and adaptation. |

"Low-Medium" at 100k is not a problem to solve — it's the nature of adversarial systems at scale. Confidence improves over time with operational experience but never reaches "proven."

**Open question scoreboard:** 32 questions total. 17 resolved (16 via simulation + ADR acceptance, Q21 ticketed). 2 launch-accepted (Q28-Q29: anchor is founder at launch, multi-anchor is Tier 3+). 4 scale questions (Q20, Q22-Q23). 8 design spikes (Q24-Q27, Q30-Q32). 1 deferred.

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

9. ~~**ADR-020 cross-reference.**~~ **[RESOLVED]** ADR-020 says continuous influence "may be revisited for variable-cost endorsements." ADR-023 resolves this question (answer: no). ADR-020 now references ADR-023 (already present in References section).
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
21. ~~**Engine sparse max-flow migration.**~~ **TICKETED (#685).** Sparse Edmonds-Karp implementation proven in simulation framework. Port to engine.
22. **Sybil mesh countermeasures.** With diversity=bridge_count proven, what additional detection beyond the diversity threshold? Options: temporal analysis (simultaneous endorsements), graph structure (dense cluster with few external connections), behavioral signals. **Ticket: #682.**
23. **Community-structure testing.** Stochastic block model graphs to re-run all scale tests. Would surface whether current thresholds need adjustment for realistic social structure. Part of #680.

---

## Design spikes needed (not yet ticketable)

These items are identified but need design work before they can be scoped into implementation tickets.

24. **Sybil detection heuristics design.** Three families identified (structural, temporal, behavioral — see scale readiness matrix Tier 3) but no concrete design exists. Questions: which specific heuristics to implement first? What thresholds? How to integrate with the trust engine (advisory flag vs hard gate)? What false positive rate is acceptable? How do heuristics interact with each other? The red team doc notes that structural heuristics force attackers to waste endorsement slots (hardest to evade), making them the highest-value first target.

25. **Multi-phase attack campaign simulation framework.** Red team doc identifies a systematic blind spot: all current simulations test single-mechanism effects on static topologies. Real attacks are multi-phase (infiltrate → establish trust → create mesh → attack). Need: a campaign builder or DSL that models sequential attacker actions with cumulative graph mutations. This is framework design, not a scenario addition.

26. **Adjudication/governance process design.** The mechanism for severe action (full disconnection, slashing) is explicitly deferred to a governance process. Questions: who can raise a motion to slash? What quorum of diverse, deeply-trusted users is required? What evidence format? What appeal process? This is a social/political design problem that can only be informed by real community norms — it's premature to design before a community exists. Likely its own ADR.

27. **Account compromise detection and response procedures.** Red team doc A2 identifies account takeover as P0. The blast radius simulation (#690) tests the *impact*, but the *response* is undesigned. Questions: how to detect behavior change post-compromise? How to revoke compromised edges and notify affected endorsers? How to restore the legitimate owner's trust position? Is there an account recovery path when root key is lost? The Ed25519 trust boundary makes this harder — there's no "password reset" equivalent.

28. **Anchor redundancy / multi-anchor design.** ~~**LAUNCH: ACCEPTED (founder as root).**~~ The anchor is a single point of failure at scale, but at launch the founder IS the trust root — this is the natural shape of bootstrapping any trust network. Multi-anchor migration becomes relevant at Tier 3+ (~10k users) when the founder can't personally vouch for the network's integrity. Questions for later: can the system support multiple anchors? What does trust mean when measured from different roots? How does anchor rotation work? See `.plan/2026-03-13-anchor-problem-statement.md`.

29. **~~Anchor-free scoring~~ → The Anchor Problem.** ~~**LAUNCH: ACCEPTED.**~~ EigenTrust/PageRank spike (2026-03-13) proved anchor-free scoring fails at Sybil detection — Sybil mesh nodes score equal to or higher than legitimate nodes. The anchor provides a critical security property (breaking self-reinforcing trust loops) that cannot be replicated without a trusted reference point. **Launch decision (2026-03-15):** founder is the trust root. The genesis-drop concern is acknowledged but pragmatically acceptable — every trust network starts from a founder's personal network. BrightID's experience validates that decentralizing trust before a user base exists creates worse problems. **Scale migration (Tier 3+):** multi-anchor consensus, rotating anchors, or community-elected anchor sets. Research sweep completed — see `.plan/2026-03-13-anchor-problem-statement.md` for full analysis and solution space.

30. **Verifiers as graph participants (identity verification + slot budget).** The trust graph already carries identity signal — when a trusted person endorses someone via QR, they're attesting to both humanity and trust. External identity verifiers (BrightID, passport services, trusted community organizers) should participate as **entities in the trust graph**, not as a separate system. The founder endorses verifier entities with high trust; verifiers endorse verified humans; those humans get graph position through the verifier's path from anchor. This is elegant: the security model is unchanged (diversity = bridge_count still holds), a compromised verifier only affects paths through that verifier, and the graph doesn't need to know what a "verifier" is — it just sees edges.

    **The core design problem is the slot budget.** A verifier entity needs to endorse potentially thousands of people, but k=10 is a core defense limiting attack surface per compromised account. Options:

    **A. Variable k per entity.** Verifiers get k=1000 or unlimited. Simple to implement (per-account parameter). But creates structurally privileged accounts — high-value targets with outsized blast radius if compromised. Mitigation: verifier endorsements carry lower weight, so compromise yields many weak edges, not many strong ones.

    **B. Light endorsement tier (general mechanism).** Everyone gets k=10 full endorsements + k=N light endorsements (lower weight, e.g. 0.1-0.2). Verifiers use the light tier. No special entity types — the mechanism is general and available to all users. Already described in trust expansion concepts as "additional endorsement tiers." Adds complexity to the endorsement model (two tiers), but the most architecturally clean option.

    **C. Verifier entities use k=10 — scale through multiple instances.** No mechanism changes. BrightID operates 100 verifier nodes, each endorsing 10 people. Mirrors real-world structure (many notaries, not one mega-notary). Scaling headache (1000 users = 100 verifier instances), but zero new concepts. Works naturally when "verifiers" are trusted humans acting as identity checkers at community events.

    **D. Room/group endorsement.** A "BrightID verification" room collectively endorses verified users (one edge from the room entity). Reuses room infrastructure. But rooms are designed for deliberation, not identity verification — conceptual overload.

    **Trade-off summary:**

    | Option | Mechanism complexity | Blast radius if compromised | Scales to 10k verified users? | Special entity types? |
    |---|---|---|---|---|
    | A. Variable k | Low (one parameter) | High (k=∞ node) | Yes | Yes |
    | B. Light endorsement tier | Medium (new edge tier) | Low (low-weight edges) | Yes | No |
    | C. Multiple k=10 instances | None | Low (k=10 per instance) | Awkward (needs 1000 instances) | No |
    | D. Room endorsement | Low (reuse rooms) | Medium (room entity) | Yes | Sort of (rooms as verifiers) |

    **Leaning:** Option B is most aligned with existing design principles (no special entities, general mechanism, bounded blast radius). Option C works naturally when verifiers are humans at community events. Option A is simplest if we accept that some entities are structurally different. The choice depends on whether verifiers are primarily automated services (favors A or B) or humans performing a role (favors C).

    **When to build:** before growth outpaces the founder's personal network. Not needed at launch.

31. **Room types: container/module separation.** The current system has one room shape (polling) with three eligibility gates. The vision requires fundamentally different interaction models (slow exchange, ranking, report synthesis, deliberation) sharing a common container. The architectural question is where the boundary between container and module lives, and what the module interface looks like. Key sub-questions:

    **a. Template vs module distinction.** Users create rooms from templates (config); developers create room types (code). Both are needed but they're different operations. How does the config_schema system work? What's configurable vs what requires a new module?

    **b. Self-hosting unit.** Is the unit of federation a whole TC instance (Mastodon model) or a single room service plugging into central identity/trust (more like OAuth + microservice)? The latter is more powerful but requires a clean API contract at the module boundary.

    **c. Private reducers.** Can a room type have a proprietary aggregation algorithm with auditable inputs/outputs? Tension with TC's transparency principle. Possible resolution: inputs and outputs are always public/auditable; the reduction function can be opaque if its correctness is verifiable (ZK proofs, deterministic replay).

    **d. Module data isolation.** Each module owns its tables. In-binary modules share Postgres (schema prefixes?). Federated modules own their storage entirely. The migration path from shared-DB to federated needs to be clean.

    **e. Lifecycle ownership.** The current `rooms__lifecycle_queue` manages poll rotation — module-specific behavior leaked into the container. In the module model, each module owns its lifecycle. The container lifecycle is simpler: room open/closed/archived.

    **Implementation path:** Extract module interface from polling code → build slow exchange as second module (proves the abstraction) → config_schema as template system → federation as HTTP API contract. See `.plan/2026-03-17-room-types-architecture.md` for full design brief.

32. **Trust engine: on-demand computation and Personalized PageRank.** The current engine pre-computes all `(anchor, user)` scores and materializes them in `trust__score_snapshots`. This creates the staleness/batch/provisional complexity that ADR-021 tries to manage. Three alternative approaches surfaced in the 2026-03-17 architecture session:

    **a. On-demand computation (same metrics).** Replace snapshot lookups with on-demand single-pair computation. Distance: Dijkstra from anchor to target, early-terminate. Diversity: single-pair max-flow. Orders of magnitude cheaper than full-graph pre-computation. Score snapshots become a cache with TTL, not the source of truth. ADR-021 cooling-off still applies to *edge creation* but score computation is always fresh.

    **b. Personalized PageRank (new metric).** PPR computes a probability distribution from a source node — "if I start random walks from myself, what fraction land on target?" Captures both distance and diversity in a single scalar. Better scaling (Monte Carlo walks are parallelizable, early-terminable, incrementally updatable). More philosophically aligned with personal trust ("how much do I trust X?" not "how much does the anchor trust X?"). But: requires re-validating all adversarial simulations against PPR instead of (distance, diversity). New ADR needed.

    **c. Hybrid gate.** PPR as fast approximate eligibility screen, exact distance/diversity as confirmation for high-trust rooms.

    **Key insight:** The pre-compute vs on-demand question is orthogonal to ADR-021. ADR-021's value is cooling-off and batch visibility for *edge creation*. Score computation doesn't need to be batched — it can always be on-demand against the current committed graph. Separating these two concerns eliminates the provisional-score complexity entirely.

    **Simulation work needed before deciding:**
    - Run adversarial test suite with PPR instead of (distance, diversity) — does PPR block Sybil meshes equally well?
    - Benchmark on-demand single-pair computation at 1k, 5k, 10k graph sizes
    - Evaluate UX: "2 hops away, 3 independent paths" (explainable) vs "trust score 0.0034" (opaque)

    **When to build:** After demo. This is a design spike that needs simulation evidence, not whiteboard discussion. The simulation harness exists for exactly this purpose.

---

## Next actions — launch ticket tracker

All tickets on the "Demo: Friends & Family (Mar 20)" milestone. Grouped by dependency and priority.

### Done (mechanism design — complete)
- [x] **Phase 1: ADR-024 accepted** — denouncer-only revocation (PR #678)
- [x] **Phase 2: ADR-023 stress-tested** — weight variance (PR #678)
- [x] **Phase 3: ADR-025 accepted** — step function decay (PR #679)
- [x] **Phase 4: Scale simulation** — BA graphs, Sybil mesh analysis, sparse max-flow (PR #684)

### Demo blockers
- [ ] **#665 → #709** — CRITICAL: verified users can't vote. Fix: self-contained constraints + `identity_verified` Layer 1 type (PR #710)
- [ ] **#656** — weight selection UI (expose ADR-023 table)
- [ ] **#657** — wire denouncement to edge revocation backend (ADR-024)
- [ ] **#658** — denouncement UI component
- [ ] **#687** — trust score dashboard polish
- [ ] **#688** — seed demo trust graph data

### Growth prerequisites (needed before scaling past demo)
- [ ] **#685** — engine sparse max-flow migration
- [ ] **#686** — engine time decay integration (ADR-025)
- [ ] **#647** — repurpose trust__user_influence table for slot model

### Scale hardening (needed for 5k-10k confidence)
- [ ] **#680** — SBM topology generation + threshold validation
- [ ] **#681** — sophisticated Sybil mesh topologies
- [ ] **#682** — correlated failure simulation
- [ ] **#690** — account compromise blast radius simulation
- [ ] **#689** — graph health monitoring dashboard

### Launch-accepted (anchor decisions — revisit at Tier 3+)
- [x] **Anchor as founder** (Q28, Q29) — founder is trust root at launch. Multi-anchor migration is Tier 3+ work. See `.plan/2026-03-13-anchor-problem-statement.md`

### Needs design spike (see Q24-Q27, Q30 above)
- [ ] Sybil detection heuristics (Q24)
- [ ] Multi-phase attack campaign simulation (Q25)
- [ ] Adjudication/governance process (Q26)
- [ ] Account compromise response procedures (Q27)
- [ ] Identity verification vs trust endorsement (Q30) — needed before adding external identity providers
- [ ] Room types: container/module separation (Q31) — design brief at `.plan/2026-03-17-room-types-architecture.md`
- [ ] Trust engine: on-demand computation + PPR evaluation (Q32) — needs simulation spike
