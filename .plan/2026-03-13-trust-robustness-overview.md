# Trust System Robustness Overview

**Date:** 2026-03-13 (revised)
**Status:** Phases 1-3 complete. Scale simulation complete. All 4 trust ADRs accepted.
**Related PRs:** #673 (GraphSpec, merged), #678 (adversarial sim + mechanism acceptance), #679 (time decay), #684 (scale simulation)

---

## 1. Scale Target & Design Philosophy

**Target:** ~100k users (friends-and-family demo → community rollout → platform growth).

**Core question:** Can you build a resilient trust system at scale with good UX for normal humans? The answer needs to work for someone who signs up via QR code at a community meeting, not just cryptography-literate early adopters.

**Honest framing:** The mechanism design is done and provably sound (diversity = bridge_count, denouncer-only revocation, step decay). The system is confidently deployable to ~5k users today. Getting beyond that is an engineering AND operational challenge — not just building more things, but operating a system under adversarial pressure. "Provably robust at 100k" is not achievable for any adversarial system. The realistic goal is bounded confidence with detection and response capability. See `.plan/2026-03-13-scale-readiness-matrix.md` for the tiered breakdown.

**Simulation-driven decisions.** Every mechanism in the trust system was accepted or rejected based on test evidence from adversarial graph simulations, not theoretical reasoning. The simulation harness runs the real `TrustEngine` against named adversarial topologies — no mocks, no approximations — and measures whether red nodes are blocked and blue nodes remain reachable. Mechanisms that look reasonable in theory but fail these tests are rejected.

**What simulation can and can't prove.** Simulation proves mechanism properties: "given this topology and these attacker actions, the system produces these scores." It cannot prove operational resilience: "when a real attacker adapts to your defenses, you'll detect and respond in time." The gap between these two is the arms race, and it's inherent to adversarial systems.

---

## 2. Trust Model Architecture

Brief summary for context; full details in `docs/domain-model.md` and the accepted ADRs.

- **Ed25519 cryptographic identity.** Each user holds a root key (cold storage) and device keys (daily use). The server is a dumb witness — it stores signed data but never handles private key material.
- **Endorsement slots (k=10, ADR-020).** Each user can endorse at most 10 others. Slots are finite and meaningful — endorsing someone is a real commitment, not a cheap click.
- **Variable-weight edges (swap method × relationship depth, ADR-023).** The weight table: QR=1.0, video=0.7, text=0.2, email=0.1. Weight bounds are enforced at the DB layer. The trust engine computes distance and diversity scores from an anchor node across these weighted edges. **Note:** These weights measure trust relationship strength, not identity confidence. Identity verification ("is this person a unique human?") is a separate concern from trust endorsement ("I vouch for this person") — see Q30 in open questions. At launch, the QR handshake implicitly conflates both; separating them is future work needed before adding external identity providers.
- **Denouncement budget (d=2, ADR-020).** Each user can denounce at most 2 people. Denouncing someone revokes your endorsement edge to them — you can't simultaneously vouch for and denounce someone.
- **Trust engine.** Computes distance + diversity scores from an anchor node. Diversity counts independent paths through distinct community members — this is the core Sybil defense.

---

## 3. Robustness Testing: What We've Proven

The simulation harness (`service/tests/common/simulation/`) runs adversarial graph topologies against the real `TrustEngine`. 31+ named scenarios across 7 topology categories. All findings below come from this harness.

### 3a. Mechanism Selection (ADR-024, Accepted 2026-03-13)

Four mechanisms were evaluated across three adversarial topologies: hub-and-spoke Sybil cluster, mercenary bot network, and colluding ring.

**Nuclear edge removal — REJECTED.** A single denouncement severs all inbound edges from the target. One malicious actor can completely disconnect a legitimate user. Weaponization is trivial.

**Score penalty — REJECTED.** Stacks linearly. 10 coordinated denouncements produce a 30.0 distance penalty that overwhelms any legitimate score. In simulation: target's distance jumps from 2.0 to 32.0, destroying eligibility. The stacking property makes it trivially weaponizable by coordinated groups regardless of per-denouncement cap design.

**Sponsorship cascade — interesting, collateral damage present.** Endorsers of denounced users are penalized (2.0 distance, -1 diversity). Effective at reaching bad actors through their endorser network, but produces 1/7 blue casualties in the mercenary scenario — legitimate users caught in cascade.

**Denouncer-only revocation — CHOSEN.** When you denounce someone, your endorsement edge to them is revoked. You can't simultaneously endorse and denounce. This is proportionate (costs the target one path, not all of them), obvious (reflects real-world meaning of "I no longer vouch for this person"), and weaponization-resistant (see 3b below).

**Coordinated denouncement test:** 3 of 4 bridge nodes denouncing a target → diversity drops from 4 to 1, target loses eligibility. The mechanism works when attackers have real edges to the target.

**Insufficient denouncement test:** 1 of 4 bridge nodes denouncing a target → diversity drops from 4 to 3, target retains eligibility. Single bad-faith denouncement cannot knock out a well-connected user.

### 3b. Weaponization Resistance

The critical property at scale: what happens when Sybil nodes mass-denounce a legitimate user?

**Score penalty (rejected mechanism):** 10 Sybil nodes denouncing a legitimate user → distance 2.0 → 32.0. Target destroyed.

**Denouncer-only revocation (chosen mechanism):** 10 Sybil nodes with no edge to target denounce them → no-op. Sybil nodes have no endorsement to revoke. Denouncement budget is spent but target is unaffected.

This is the key property that makes the system safe at 100k. An attacker cannot weaponize denouncement against users they don't already endorse. To acquire those endorsement slots, they must infiltrate the legitimate trust graph — which is precisely what the slot budget and diversity metric are designed to prevent.

### 3c. Denouncement Propagation (Cascade)

The sponsorship cascade is the one-hop consequence of denouncement: endorsers of denounced users are penalized.

**Depth:** One-hop only. Tested explicitly — no propagation beyond direct endorsers. A denouncement against Bob penalizes Alice (who endorsed Bob) but not Carol (who endorsed Alice).

**Circular cascade risk:** Tested on ring topology A→B→C→A. No runaway accumulation. The engine does not follow cycles — each node is visited once per anchor computation.

**Penalty operating point:** 2.0 distance / -1 diversity is the selected value. Penalty sweep (1.0 through 4.0) was run against all adversarial topologies. Higher values increase blue casualties without improving red blocking. 2.0/1 is the point where the loss function is minimized: bad actors reliably lose eligibility, legitimate users caught in cascade retain a remediation path (seek a fresh endorsement from someone who actually knows them).

**Loss function bias:** False negatives (bad actors passing) are weighted much more costly than false positives (legitimate users temporarily downgraded). Cascade collateral is acceptable. The asymmetry narrows at scale as trust status carries more real-world consequence — that's when the threshold shifts toward due process.

### 3d. Weight Variance Stress Testing (ADR-023)

ADR-023 accepted the structural decision (swap method × relationship depth determines weight) with provisional values. Simulation confirmed the values are safe.

**Mixed-weight scenarios:** ADR-023 table values (QR=1.0, video=0.7, text=0.4, email=0.2) applied across all adversarial topologies. Outcome unchanged — red blocked, blue passes — across the realistic weight distribution.

**Weight sweep:** 0.1 → 1.0 on the mercenary-bot scenario. Red nodes blocked at all weight levels. Weight affects score magnitude but not the blocking/passing outcome.

**Max-weight Sybil:** Sybil cluster with all edges at weight 1.0 still fails the diversity check. Diversity=1 regardless of weight magnitude. The diversity metric counts distinct paths through distinct community members — high-weight edges from a single cluster don't count as multiple independent paths.

**Conclusion:** The combination of diversity metric + endorsement slot budget + weight cap is robust to weight manipulation.

### 3e. Property-Based Testing (PR #673)

PR #673 introduced infrastructure for systematic invariant checking:

- **GraphSpec** enables pure graph construction without database — faster test iteration and decoupled from persistence.
- **Proptest integration** for fuzzing graph invariants across randomly-generated topologies.
- **Behavioral predicates:** `red_nodes_blocked`, `blue_nodes_reachable`, `no_single_denounce_changes_blue_eligibility` — these run against every generated graph.
- **Temporal extensions** — edge age modeling and decay scaffolding for Phase 3.

---

## 4. Scale Analysis: Mechanism Properties and Confidence Tiers

The mechanisms below have provable, scale-invariant properties. The mechanism math is complete.

| Mechanism | Scale Property | Status |
|---|---|---|
| Endorsement slots (k=10) | O(k·N) edges, bounded per-user. At 100k: max 1M edges | Accepted (ADR-020) |
| Variable weights | Diversity metric bounds gaming regardless of weight values | Accepted (ADR-023) |
| Denouncer-only revocation | Per-user budget (d=2) limits coordinated attacks. 100k users → 200k max denouncements total, but each costs attacker a slot | Accepted (ADR-024) |
| One-hop cascade | Linear in edge count, bounded by slot limit | Accepted (ADR-024) |
| Time decay | Passive Sybil resistance without user action. Step function: 1.0/0.5/0.0 at 1yr/2yr. | Accepted (ADR-025) |

**Key scale insight:** The diversity metric is the core Sybil defense. Proven: diversity = bridge_count (Sybil mesh members achieve diversity exactly equal to the number of compromised bridge nodes on independent paths from anchor). Internal mesh endorsements cannot inflate diversity. This means Sybil resistance reduces to: "how hard is it to compromise 2+ accounts on genuinely independent paths?"

**What this insight does NOT guarantee:** The *cost* of compromising 2 accounts decreases with population size (price-to-sell is distributed; more users = cheaper tail). Account takeover bypasses the endorsement ceremony entirely. The mechanism math is sound, but the security of the system at scale depends on detection and response — not just the mechanism properties. See `.plan/2026-03-13-red-team-threat-model.md` for the full attacker-perspective analysis.

**Scale confidence tiers:**

| Scale | Confidence | What it means |
|---|---|---|
| 1k–5k | **High** | Mechanism security sufficient. Completable engineering work. |
| 5k–10k | **Medium** | Need sparse max-flow + monitoring. Engineering + initial operations. |
| 10k–100k | **Low-Medium** | Need heuristic detection + response capability. Ongoing operations, never "done." |

---

## 5. What's Needed Beyond Mechanism Design

The mechanism design is complete. What remains falls into two categories: **finite engineering** (completable) and **ongoing operations** (never done).

### Finite engineering (completable)

| Work item | Why needed | Ticket |
|---|---|---|
| Sparse max-flow in engine | Dense O(n²) matrix hits memory wall at ~5k nodes | #685 |
| Community-structure topology testing | BA graphs are unrealistically well-connected; need SBM to validate thresholds | #680 |
| Correlated failure testing | Real communities cluster harder than BA; test cohort decay on realistic topology | #682 |
| Sybil detection heuristics | Diversity threshold alone is insufficient at scale; need structural/temporal/behavioral detection | #682 |
| Batch reconciliation profiling | Unknown performance at 10k+ nodes | Not yet filed |

### Ongoing operations (never done)

| Capability | Why needed | When needed |
|---|---|---|
| Graph health monitoring | Detect topology changes, anomalous endorsement patterns, diversity shifts | From ~5k users |
| Incident response | Detect → investigate → respond → learn cycle for novel attacks | From ~5k users |
| Heuristic tuning | Attackers adapt to detection; heuristics must evolve | Continuous once deployed |
| Governance/adjudication | Severe action (slashing, disconnection) requires human judgment, not automation | When trust status carries real-world consequences |
| Cost-curve monitoring | Empirical Sybil entry cost vs theoretical model; validate security assumptions | From ~10k users |
| Account compromise response | Detect compromised accounts, revoke edges, notify endorsers, restore owner | From ~10k users |
| Identity verification infrastructure | Separate "unique human" verification from trust endorsement; support diverse verifiers | Before adding external identity providers |

These are not oversights — they are the reality of operating any adversarial system at scale. The mechanism design is clean and provable. The operational work is adaptive and ongoing. Both are necessary.

**Anchor at launch:** The founder is the trust root. This is pragmatic — every trust network starts from a founder's personal network. Multi-anchor migration is Tier 3+ work. See `.plan/2026-03-13-anchor-problem-statement.md`.

---

## 6. Roadmap

### Completed

- **Phase 1: Denouncement mechanism** — ADR-024 accepted. Denouncer-only revocation validated with 31 adversarial scenarios (PR #678).
- **Phase 2: Weight variance** — ADR-023 stress-tested. Max-weight Sybil still fails diversity (PR #678).
- **Phase 3: Time decay** — ADR-025 accepted. Step function (1.0/0.5/0.0 at 1yr/2yr), auto-release below 0.05, renewal = re-swap (PR #679).
- **Phase 4: Scale simulation** — BA graphs at 1k-10k, Sybil mesh analysis, sparse max-flow proven, bridge removal tested (PR #684).

### Near-term (pre-launch — finite, completable)

- **Weight UI (#656):** Swap method + relationship depth selection in endorsement flow. Currently the weight is hardcoded; this exposes the ADR-023 table to users.
- **Trust dashboard polish:** Score card, eligibility messaging, remediation path clarity.
- **Seeded demo data:** Pre-populated trust graph for demo users so new visitors see a live, realistic network rather than an empty graph.

### Scale hardening (pre-10k — finite, completable)

- **Community-structure topology testing (#680)** — SBM graphs to validate thresholds on realistic topology
- **Engine sparse max-flow migration (#681)** — port sparse Edmonds-Karp to `max_flow.rs`
- **Correlated failure scenarios (#682)** — realistic cohort decay, Sybil detection heuristics
- **Graph health monitoring** — dashboard for distance, diversity, reachability, edge creation rate

### Post-launch (ongoing, never done)

- **Operational security** — monitoring, detection, incident response. This is not a deliverable; it's a mode of operation that begins at ~5k users and continues indefinitely.
- **Governance process design** — adjudication for severe cases (its own ADR, substantial design work). Can only be designed once a real community exists.
- **Heuristic evolution** — detection heuristics must adapt as attackers learn to evade them.
- **Arms race management** — novel attacks will emerge. The response loop is: detect → mitigate → add simulation scenario → harden → repeat.

---

## 7. Testing Infrastructure

The simulation framework in `service/tests/common/simulation/`:

| Component | Purpose |
|---|---|
| `GraphBuilder` | DB-backed graph construction with full schema validation |
| `GraphSpec` | Pure in-memory graph construction, no database required |
| `SimulationReport` | Runs the real TrustEngine and captures score measurements |
| `MechanismComparison` | Evaluates multiple mechanisms across the same topology |
| `ComparisonTable` | Formats mechanism comparison results for test output |
| Behavioral predicates | `red_nodes_blocked`, `blue_nodes_reachable`, `no_single_denounce_changes_blue_eligibility` |
| Proptest integration | Property-based fuzzing across randomly-generated graph topologies |
| Temporal extensions | Edge age modeling and decay scaffolding (Phase 3 ready) |

**Test count:** 31+ named adversarial scenarios. Coverage spans: hub-and-spoke Sybil, mercenary bot network, colluding ring, fully-connected cluster, coordinated/insufficient denouncement, weight sweeps, penalty sweeps, circular cascade, one-hop propagation depth.

**Design principle:** The harness runs the real engine. There are no stubs or mock trust computations. If a mechanism looks correct in tests, it means the actual engine produced the expected scores — not a simulation of the engine.
