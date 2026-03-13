# Scale Readiness Matrix

**Date:** 2026-03-13 (revised)
**Purpose:** Map scale milestones to evidence requirements and operational readiness. Each tier earns its confidence through tested results, not extrapolation from smaller tiers.

---

## What this matrix is — and what it isn't

**This is a checklist of things to build**, not a certification that the system is safe. Passing all gates at a tier means "we've tested the attacks we can think of and they don't work." It does NOT mean "the system is secure against all attacks at this scale."

Every deployed adversarial system — PageRank, Bitcoin, PKI, social media moderation — has been an ongoing arms race. None were proven robust before deployment. They launched with bounded confidence, got attacked in ways nobody predicted, and evolved. This system will follow the same pattern.

**The honest framing:**

| Tier range | Nature of work | Completable? |
|---|---|---|
| **Tiers 0-1** (to 1k) | Build and verify | **Yes.** Finite engineering work. Ship with confidence. |
| **Tiers 2-3** (to 10k) | Build, verify, and instrument | **Yes** as engineering. Ship with monitoring. |
| **Tiers 4-5** (to 100k) | Ongoing operations | **Never.** Ship with monitoring + response capability + acceptance that you'll be adapting. |

The distinction is between **mechanism security** (mathematical, provable, completable — e.g., diversity = bridge_count) and **operational security** (adaptive, continuous, never done — e.g., detecting novel attack patterns, responding to account compromises). Mechanism security is done. Operational security is ongoing work that scales sublinearly with user count but never hits zero.

## How to read gate criteria

Each scale tier defines:
- **Gate criteria** — what must be true before operating at this scale
- **Must-pass tests** — automated simulation scenarios with specific assertions
- **Evidence artifacts** — PRs, ADRs, simulation output that prove the gate is met
- **Known risks accepted** — things explicitly deferred with reasoning documented
- **Operational requirements** (Tiers 3+) — monitoring, response, and ongoing work needed at this scale

A tier is **PASS** when all gate criteria have evidence. **BLOCKED** when work is identified but incomplete. **NOT STARTED** when the work hasn't been scoped or begun. Note: even **PASS** tiers carry residual risk — attackers will find things we haven't tested.

---

## Tier 0: Demo (100 users)

**Status: PASS**

| Gate criteria | Evidence | Status |
|---|---|---|
| Core flow works end-to-end | Manual testing, smoke tests, QR endorsement flow (#618) | Done |
| Trust engine produces correct scores | 31+ adversarial simulation scenarios (PR #678) | Done |
| Denouncement revokes endorser's edge only | ADR-024 accepted with simulation evidence | Done |
| Time decay applies step function | ADR-025 accepted with simulation evidence (PR #679) | Done |

**Known risks accepted:** Dense max-flow matrix is fine at n=100 (40KB). No monitoring. No Sybil detection beyond diversity threshold. At this scale, the attack surface is small enough that mechanism security alone is sufficient — operational security is not yet needed.

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
- No Sybil detection heuristics beyond diversity.
- **Account compromise vector exists.** The QR handshake requires physical copresence for endorsement *creation*, but existing edges persist after account compromise. An attacker who gains control of an already-endorsed account inherits its graph position without needing to pass any endorsement ceremony. Decay partially mitigates this (compromised-then-abandoned accounts lose weight), but there's a window. At 1k, the number of compromisable accounts is small enough that this is acceptable.
- **Sybil entry cost decreases with population.** If willingness to sell an endorsement is normally distributed, the cost of the cheapest 2 accounts on independent paths falls as N grows. At 1k this cost is still meaningfully high.

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
| Sybil detection: structural heuristic | At least one graph-structural heuristic implemented and tested. See heuristic layer below. | Not started |
| Sybil detection: temporal heuristic | At least one temporal-pattern heuristic implemented and tested. See heuristic layer below. | Not started |
| Account compromise threat modeled | Simulation of account takeover scenarios: attacker inherits existing endorsed account's graph position. Measure blast radius and detection feasibility. | Not started |
| 10k SBM simulation passes Tier 2 tests | Re-run all Tier 2 must-pass tests at 10k. Verify thresholds still hold. | Not started |
| Trust graph health monitoring | Dashboard showing: mean distance, diversity distribution, reachability %, edge creation rate, denouncement rate, decay churn rate. | Not started |

### Sybil detection heuristic layer

The diversity threshold alone is insufficient at scale. As population grows, the cost of buying/compromising 2 accounts on independent paths decreases (price distribution effect). The heuristic layer raises the effective cost by detecting suspicious patterns that legitimate users don't produce.

**Three heuristic families (at least one from each of the first two required for this tier):**

**1. Structural signals** (graph topology) — hardest to evade
- Dense subgraph with few external connections (classic Sybil cluster signature)
- High reciprocity ratio within a subgroup (A→B and B→A for most edges; legitimate networks are less symmetric)
- Bridge concentration: subgraph where all external paths funnel through 1-2 nodes
- Low conductance cuts suggesting artificial community boundaries
- *Why hard to evade:* attacker must "waste" endorsement slots (k=10 cap) on edges that don't serve the attack, just to look legitimate. Directly conflicts with attack goal of maximizing Sybil diversity.

**2. Temporal signals** (when endorsements happen) — moderate evasion cost
- Burst detection: N endorsements to/from an account within a short window
- Synchronized creation: multiple accounts created in the same window that later cross-endorse
- Age-diversity mismatch: account is 2 days old but has 5 endorsements (suspicious velocity)
- *Why moderate evasion:* attacker can space out actions over weeks, but this increases attack duration and cost. Time decay works in the defender's favor — slow attacks risk edges decaying before the mesh is complete.

**3. Behavioral signals** (how accounts are used) — highest evasion cost
- Endorsement-only accounts: endorse but never vote or participate
- Uniform behavior: Sybil accounts act identically (same voting patterns, timing)
- Endorsement pattern mismatch: endorses people they never interact with on the platform
- *Why highest evasion:* attacker must simulate realistic platform usage across all Sybil accounts, which costs real human time per account.

**Must-pass tests (to be designed):**
- All Tier 2 tests at 10k scale
- `scale_reconciliation_benchmark_10k` — full recompute completes in <10s
- `scale_sybil_dense_subgraph_detection_10k` — Sybil mesh of 20 nodes attached via 3 bridges is flagged by structural heuristic
- `scale_sybil_temporal_burst_detection_10k` — 10 endorsements from new accounts in 1 hour are flagged
- `scale_account_compromise_blast_radius_10k` — attacker takes over top-3 betweenness-centrality accounts; measure how many Sybil nodes gain eligibility
- `scale_incremental_update_10k` — single edge change triggers partial recompute, not full graph

**Key uncertainty:** The DB-based trust engine (recursive CTE for Dijkstra, `FlowGraph` for max-flow) may need architectural changes at this tier. The simulation framework computes everything in-memory with sparse data structures. The engine computes via SQL + in-memory dense matrix. The gap between "simulation proves the math works" and "engine implements it efficiently" widens here.

**Operational requirements (begins here):**
- Trust graph health dashboard (mean distance, diversity distribution, reachability %, edge creation rate, denouncement rate)
- Anomaly alerts for sudden topology changes
- On-call response procedure for detected attacks
- Regular (weekly) graph health review

This is where the work shifts from "build and ship" to "build, ship, and operate." The engineering deliverables are finite, but operating the system is ongoing.

---

## Tier 4: Platform (50k users)

**Status: NOT STARTED — and may never be "PASS" in the traditional sense**

This tier is fundamentally different from Tiers 0-3. It cannot be completed through simulation and engineering alone. It requires operational experience, real-world attack data, and governance processes that can only be designed once a real community exists. The gate criteria below are necessary conditions, not sufficient ones.

| Gate criteria | Evidence needed | Status |
|---|---|---|
| Governance/adjudication process exists | ADR for severe action (full disconnection, slashing). Quorum design, evidence format, appeal process. | Not started |
| Recovery playbook documented | Procedures for: detected Sybil attack, compromised bridge node, mass edge decay, administrative edge revocation. | Not started |
| Anomaly detection operational | Automated alerts for: sudden topology changes, unusual endorsement patterns, diversity distribution shifts. | Not started |
| 50k SBM simulation passes all tests | All Tier 2-3 tests at 50k. Verify no emergent behavior at scale. | Not started |
| Sybil cost analysis completed | Empirical estimate: at 50k users, how many accounts are "cheaply" compromisable? Do they cluster on independent paths? Model the cost distribution — if price-to-sell is normal, what does the tail look like? | Not started |
| Account compromise response procedures | Defined process for: detecting compromised accounts, revoking their edges, notifying affected endorsers, restoring legitimate owner's trust position. | Not started |
| Incident response exercised | At least one real or simulated incident has been detected, investigated, and resolved using the playbook. | Not started |

**Key uncertainty:** This tier introduces governance questions that simulation alone cannot answer. The adjudication process for severe slashing is a social/political design problem, not a technical one. The "right" quorum, evidence standard, and appeal process depend on community norms that don't exist yet at demo scale. Additionally, the Sybil cost analysis is partly empirical — it depends on the actual user population's behavior and economic incentives, which can't be fully modeled in simulation.

**Operational requirements (ongoing, never "done"):**
- All Tier 3 operational requirements, plus:
- Incident response capability (detect → investigate → respond → learn)
- Regular review of heuristic effectiveness (are detection rates holding? are attackers adapting?)
- Cost-curve monitoring: is the empirical cost of Sybil entry tracking the theoretical model?
- Community governance processes actually functioning (not just designed)

---

## Tier 5: Target (100k users)

**Status: NOT STARTED — this tier is an ongoing operational commitment, not a deliverable**

"100k users" is not a finish line. It is a scale at which the system requires continuous, active maintenance of trust properties. The gate criteria below describe the *minimum conditions* for operating at this scale with bounded confidence. Passing them does not mean the system is "proven robust" — it means you've tested the attacks you can think of, built detection for suspicious patterns, and have the operational capability to respond when something new emerges.

Every real-world trust/reputation system at this scale (PageRank, Bitcoin, PKI, Reddit karma, Twitter verification) has been an ongoing arms race. This system will be too. The realistic expectation: the amount of active work decreases over time as attack classes get covered and heuristics improve, but it never hits zero.

| Gate criteria | Evidence needed | Status |
|---|---|---|
| All Tier 0-4 gates pass | Cumulative evidence from all lower tiers | Not started |
| Red team exercise completed | Adversary with budget (N accounts, M endorsements) attempts infiltration on test network. Measures actual attack cost vs. theoretical. Must test both account creation AND account compromise vectors. | Not started |
| Sybil cost curve validated | Empirical measurement: at 100k, what does it actually cost to get 2 accounts on independent paths? Compare against theoretical model from Tier 4 cost analysis. | Not started |
| Real topology validation | Test against at least one real social graph dataset (e.g., ego-network data). Verify simulation predictions match. | Not started |
| Distance threshold validated or tightened | With community-structure data: confirm 5.0 is appropriate or tighten to reduce Sybil attack surface. | Not started |
| Formal diversity=bridge_count argument documented | Written proof (or verified test coverage) that Menger's theorem guarantee holds for the specific vertex-split construction used in the engine. | Not started |
| Heuristic detection layer validated against adaptive attacker | Red team exercise includes an attacker who knows the heuristics and actively evades them. Measures residual detection rate. | Not started |

**Key uncertainty:** The gap between synthetic topology testing and real-world behavior. Even SBM graphs are idealized. Real social networks have: degree-correlated clustering, temporal burstiness (people join in waves), geographic/cultural community structure, and adversarial actors who adapt. The red team exercise is the closest proxy for real-world confidence. Additionally, account compromise at scale (credential stuffing, phishing, purchased accounts) is a proven attack vector in other platforms — the trust boundary (Ed25519 keys, no server-side private material) makes it harder but not impossible.

**The arms race reality:** Attackers adapt. A heuristic that catches 90% of Sybils today will catch fewer once attackers learn to evade it. New attack vectors will emerge that no simulation predicted. The system's long-term health depends not on pre-deployment testing but on the feedback loop: detect → respond → harden → repeat. This tier is not a gate you pass; it's a mode you operate in.

---

## Cross-cutting risks

These risks span multiple tiers and could force revisiting earlier gates:

| Risk | Impact | Mitigation | First testable at |
|---|---|---|---|
| **Community-structure topology breaks diversity** | Nodes in tight clusters may have diversity=1 (all paths go through same bridge). Diversity threshold of 2 would exclude them. | Test with SBM. If widespread, either lower threshold (accepting more Sybil risk) or incentivize cross-community endorsements. | Tier 2 |
| **Decay pressure fragments communities** | If most endorsements are intra-community and the community goes inactive, all edges decay simultaneously. | Correlated decay testing on SBM. If severe, consider community-level renewal or longer decay window. | Tier 2 |
| **Sybil entry cost decreases with population** | If willingness to sell/compromise an endorsement is normally distributed, the cheapest 2 accounts on independent paths get cheaper as N grows. Diversity threshold sets *how many* you need, but not the per-unit cost. | Heuristic detection layer raises effective cost. Structural heuristics force attacker to waste limited slots (k=10) on non-attack edges. Behavioral signals require human-time investment per Sybil. Red team exercise at Tier 5 to empirically measure cost curve. | Tier 3 (heuristics), Tier 5 (empirical) |
| **Account compromise bypasses endorsement ceremony** | QR handshake ensures endorsement *creation* requires copresence, but account takeover inherits existing edges. Attacker gains graph position without any endorsement ceremony. | Credential-layer defenses (Ed25519 keys, no server-side private key material). Decay mitigates stale compromised accounts. Anomaly detection for behavior change post-compromise. Simulate blast radius of high-centrality account takeover. | Tier 3 |
| **Engine architecture doesn't scale** | SQL-based path computation + dense max-flow may need replacement, not just optimization. | Profile at each tier. If DB approach hits wall, consider in-memory graph engine (like simulation framework). | Tier 3 |
| **Sybil strategy evolves** | Attackers adapt to diversity threshold by compromising real accounts rather than creating fake ones. At scale this is cheaper than social engineering new endorsements. | Heuristic detection layer + behavioral signals. Red team at Tier 5. Accept that sophisticated, well-funded attackers will always be able to compromise some accounts — the question is whether the cost exceeds the value of the attack. | Tier 3 (heuristics), Tier 5 (red team) |
| **Governance deadlock** | Adjudication quorum too high → can't act on bad actors. Too low → weaponizable. | Governance design ADR. Simulate quorum scenarios. | Tier 4 |

---

## Using this matrix

**Before claiming a scale target:** Check which tier that target falls in. If the tier isn't PASS, enumerate what's BLOCKED and what's NOT STARTED. Don't extrapolate from lower tiers — each tier exists because the previous tier's evidence doesn't cover it. Even at PASS tiers, the claim is "we've tested what we can think of," not "the system is safe."

**When closing a work item:** Update the relevant gate criteria status and link the evidence (PR number, test name, ADR reference). A gate is PASS only when evidence exists, not when we believe it will pass.

**When a test fails:** If a must-pass test fails at a higher tier, that tier is BLOCKED until either (a) the issue is fixed or (b) the gate criteria and risk acceptance are explicitly revised with reasoning.

**When an attack succeeds in production:** This is expected. The response is: (1) mitigate the immediate impact, (2) add a simulation scenario reproducing the attack, (3) add detection heuristics, (4) update this matrix if gate criteria were insufficient. The matrix is a living document, not a static certification.
