# Scale Readiness Matrix

**Date:** 2026-03-13
**Purpose:** Map scale milestones to evidence requirements. Each tier earns its confidence through tested results, not extrapolation from smaller tiers.

---

## How to read this

Each scale tier defines:
- **Gate criteria** — what must be true before operating at this scale
- **Must-pass tests** — automated simulation scenarios with specific assertions
- **Evidence artifacts** — PRs, ADRs, simulation output that prove the gate is met
- **Known risks accepted** — things explicitly deferred with reasoning documented

A tier is **PASS** when all gate criteria have evidence. **BLOCKED** when work is identified but incomplete. **NOT STARTED** when the work hasn't been scoped or begun.

---

## Tier 0: Demo (100 users)

**Status: PASS**

| Gate criteria | Evidence | Status |
|---|---|---|
| Core flow works end-to-end | Manual testing, smoke tests, QR endorsement flow (#618) | Done |
| Trust engine produces correct scores | 31+ adversarial simulation scenarios (PR #678) | Done |
| Denouncement revokes endorser's edge only | ADR-024 accepted with simulation evidence | Done |
| Time decay applies step function | ADR-025 accepted with simulation evidence (PR #679) | Done |

**Known risks accepted:** Dense max-flow matrix is fine at n=100 (40KB). No monitoring. No Sybil detection beyond diversity threshold.

---

## Tier 1: Early Adopters (1k users)

**Status: PASS**

| Gate criteria | Evidence | Status |
|---|---|---|
| Graph stays fully connected at 1k | BA simulation: 100% reachable, mean distance 2.5, max 4.0 (PR #684) | Done |
| Min diversity ≥ 3 for all nodes | BA simulation at 1k with m=3: min diversity = 3 (PR #684) | Done |
| Sybil mesh cannot inflate diversity | diversity = bridge_count proven across 1-5 bridge scenarios (PR #684) | Done |
| Bridge removal doesn't fragment graph | 3 highest-degree nodes removed: 99.7% still reachable (PR #684) | Done |
| Correlated decay is survivable | 100-node cohort aged 2yr+: 0 unreachable (PR #684) | Done |
| All 4 trust ADRs accepted | ADR-020, 023, 024, 025 all accepted | Done |

**Known risks accepted:**
- BA topology is unrealistically well-connected. Real social graphs will have more clustering and fewer independent paths. This is the primary risk carried forward.
- Dense max-flow at 1k uses ~4MB — acceptable.
- No Sybil detection heuristics beyond diversity. At 1k, compromising 2 independent accounts requires real social engineering of 2 separate people — meaningfully hard.

---

## Tier 2: Community (5k users)

**Status: BLOCKED on #680, #681**

| Gate criteria | Evidence needed | Status |
|---|---|---|
| Engine handles 5k-node diversity computation | Sparse max-flow deployed in engine (`max_flow.rs`). Dense matrix hits 100MB at 5k. | BLOCKED on #681 |
| Thresholds hold on community-structure topology | SBM graphs at 5k tested: distance/diversity distributions, eligibility fraction, Sybil resistance | BLOCKED on #680 |
| Correlated decay survivable in clustered graphs | SBM cohort decay test: communities with high intra-density don't fragment | BLOCKED on #682 |
| Bridge nodes between communities are resilient | SBM bridge removal: measure inter-community connectivity after targeted bridge compromise | BLOCKED on #680 |

**Must-pass tests (to be implemented):**
1. `scale_sbm_distance_distribution_5k` — SBM graph with 10 communities, 5k nodes. Assert: ≥95% reachable, mean distance < 4.0.
2. `scale_sbm_diversity_distribution_5k` — Same graph. Assert: median diversity ≥ 2, ≥80% nodes eligible at (5.0, 2).
3. `scale_sbm_sybil_mesh_5k` — Sybil mesh attached to SBM graph. Assert: diversity = bridge_count still holds.
4. `scale_sbm_correlated_decay_5k` — 50-node intra-community cohort aged 2yr+. Assert: ≤5% of cohort becomes unreachable.
5. `scale_sbm_bridge_removal_5k` — Remove top 3 inter-community bridge nodes. Assert: ≥90% still reachable.

**Work items:**
- #680: Community-structure topology generation (SBM graphs)
- #681: Engine sparse max-flow migration
- #682: Correlated failure scenarios with realistic clustering

**Open questions that could change thresholds:**
- If SBM tests show diversity < 2 for many nodes, the diversity threshold may need to drop to 1 (losing Sybil resistance) or the endorsement flow needs to encourage cross-community connections.
- If distance exceeds 5.0 for inter-community paths, the distance threshold needs relaxing or the weight table needs adjustment.

---

## Tier 3: Growth (10k users)

**Status: NOT STARTED**

| Gate criteria | Evidence needed | Status |
|---|---|---|
| Batch reconciliation scales to 10k | Profile `recompute_from_anchor` at 10k. If >10s, need incremental update (recompute affected subgraph only). | Not started |
| Dijkstra path computation scales | Profile DB-based recursive CTE at 10k. May need to move path-finding out of SQL or add graph-aware indices. | Not started |
| Sybil detection heuristics exist | At least one heuristic beyond diversity threshold: temporal burst detection OR structural cluster detection. | Not started |
| 10k SBM simulation passes Tier 2 tests | Re-run all Tier 2 must-pass tests at 10k. Verify thresholds still hold. | Not started |
| Trust graph health monitoring | Dashboard showing: mean distance, diversity distribution, reachability %, edge creation rate, denouncement rate, decay churn rate. | Not started |

**Must-pass tests (to be designed):**
- All Tier 2 tests at 10k scale
- `scale_reconciliation_benchmark_10k` — full recompute completes in <10s
- `scale_sybil_temporal_detection_10k` — burst-created Sybil edges are flagged by temporal heuristic
- `scale_incremental_update_10k` — single edge change triggers partial recompute, not full graph

**Key uncertainty:** The DB-based trust engine (recursive CTE for Dijkstra, `FlowGraph` for max-flow) may need architectural changes at this tier. The simulation framework computes everything in-memory with sparse data structures. The engine computes via SQL + in-memory dense matrix. The gap between "simulation proves the math works" and "engine implements it efficiently" widens here.

---

## Tier 4: Platform (50k users)

**Status: NOT STARTED**

| Gate criteria | Evidence needed | Status |
|---|---|---|
| Governance/adjudication process exists | ADR for severe action (full disconnection, slashing). Quorum design, evidence format, appeal process. | Not started |
| Recovery playbook documented | Procedures for: detected Sybil attack, compromised bridge node, mass edge decay, administrative edge revocation. | Not started |
| Anomaly detection operational | Automated alerts for: sudden topology changes, unusual endorsement patterns, diversity distribution shifts. | Not started |
| 50k SBM simulation passes all tests | All Tier 2-3 tests at 50k. Verify no emergent behavior at scale. | Not started |
| Sybil cost analysis completed | Empirical estimate: at 50k users, how many accounts are "cheaply" compromisable? Do they cluster on independent paths? | Not started |

**Key uncertainty:** This tier introduces governance questions that simulation alone cannot answer. The adjudication process for severe slashing is a social/political design problem, not a technical one. The "right" quorum, evidence standard, and appeal process depend on community norms that don't exist yet at demo scale.

---

## Tier 5: Target (100k users)

**Status: NOT STARTED**

| Gate criteria | Evidence needed | Status |
|---|---|---|
| All Tier 0-4 gates pass | Cumulative evidence from all lower tiers | Not started |
| Red team exercise completed | Adversary with budget (N accounts, M endorsements) attempts infiltration on test network. Measures actual attack cost vs. theoretical. | Not started |
| Real topology validation | Test against at least one real social graph dataset (e.g., ego-network data). Verify simulation predictions match. | Not started |
| Distance threshold validated or tightened | With community-structure data: confirm 5.0 is appropriate or tighten to reduce Sybil attack surface. | Not started |
| Formal diversity=bridge_count argument documented | Written proof (or verified test coverage) that Menger's theorem guarantee holds for the specific vertex-split construction used in the engine. | Not started |

**Key uncertainty:** The gap between synthetic topology testing and real-world behavior. Even SBM graphs are idealized. Real social networks have: degree-correlated clustering, temporal burstiness (people join in waves), geographic/cultural community structure, and adversarial actors who adapt. The red team exercise is the closest proxy for real-world confidence.

---

## Cross-cutting risks

These risks span multiple tiers and could force revisiting earlier gates:

| Risk | Impact | Mitigation | First testable at |
|---|---|---|---|
| **Community-structure topology breaks diversity** | Nodes in tight clusters may have diversity=1 (all paths go through same bridge). Diversity threshold of 2 would exclude them. | Test with SBM. If widespread, either lower threshold (accepting more Sybil risk) or incentivize cross-community endorsements. | Tier 2 |
| **Decay pressure fragments communities** | If most endorsements are intra-community and the community goes inactive, all edges decay simultaneously. | Correlated decay testing on SBM. If severe, consider community-level renewal or longer decay window. | Tier 2 |
| **Engine architecture doesn't scale** | SQL-based path computation + dense max-flow may need replacement, not just optimization. | Profile at each tier. If DB approach hits wall, consider in-memory graph engine (like simulation framework). | Tier 3 |
| **Sybil strategy evolves** | Attackers adapt to diversity threshold by compromising real accounts rather than creating fake ones. | Sybil cost analysis + red team at Tier 4-5. May need behavioral signals beyond graph structure. | Tier 4 |
| **Governance deadlock** | Adjudication quorum too high → can't act on bad actors. Too low → weaponizable. | Governance design ADR. Simulate quorum scenarios. | Tier 4 |

---

## Using this matrix

**Before claiming a scale target:** Check which tier that target falls in. If the tier isn't PASS, enumerate what's BLOCKED and what's NOT STARTED. Don't extrapolate from lower tiers — each tier exists because the previous tier's evidence doesn't cover it.

**When closing a work item:** Update the relevant gate criteria status and link the evidence (PR number, test name, ADR reference). A gate is PASS only when evidence exists, not when we believe it will pass.

**When a test fails:** If a must-pass test fails at a higher tier, that tier is BLOCKED until either (a) the issue is fixed or (b) the gate criteria and risk acceptance are explicitly revised with reasoning.
