# Trust System Robustness Overview

**Date:** 2026-03-13
**Status:** Phase 1 + Phase 2 complete; Phase 3 in progress
**Related PRs:** #673 (GraphSpec, merged), #678 (adversarial sim + mechanism acceptance)

---

## 1. Scale Target & Design Philosophy

**Target:** ~100k users at launch (friends-and-family demo to full community rollout).

**Core question:** Can you build a resilient trust system at scale with good UX for normal humans? The answer needs to work for someone who signs up via QR code at a community meeting, not just cryptography-literate early adopters.

**Simulation-driven decisions.** Every mechanism in the trust system was accepted or rejected based on test evidence from adversarial graph simulations, not theoretical reasoning. The simulation harness runs the real `TrustEngine` against named adversarial topologies — no mocks, no approximations — and measures whether red nodes are blocked and blue nodes remain reachable. Mechanisms that look reasonable in theory but fail these tests are rejected.

**Mechanisms are scale-specific.** The current set (endorsement slots, variable weights, denouncer-only revocation, one-hop cascade) targets the 100k threshold. Different parameters or additional mechanisms will be needed as the network grows past that — those are tracked in the "deferred past 100k" section below.

---

## 2. Trust Model Architecture

Brief summary for context; full details in `docs/domain-model.md` and the accepted ADRs.

- **Ed25519 cryptographic identity.** Each user holds a root key (cold storage) and device keys (daily use). The server is a dumb witness — it stores signed data but never handles private key material.
- **Endorsement slots (k=10, ADR-020).** Each user can endorse at most 10 others. Slots are finite and meaningful — endorsing someone is a real commitment, not a cheap click.
- **Variable-weight edges (swap method × relationship depth, ADR-023).** The weight table: QR=1.0, video=0.49, text=0.2, email=0.1. Weight bounds are enforced at the DB layer. The trust engine computes distance and diversity scores from an anchor node across these weighted edges.
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

**Mixed-weight scenarios:** ADR-023 table values (QR=1.0, video=0.49, text=0.2, email=0.1) applied across all adversarial topologies. Outcome unchanged — red blocked, blue passes — across the realistic weight distribution.

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

## 4. Scale Analysis: What Works at 100k

| Mechanism | Scale Property | Status |
|---|---|---|
| Endorsement slots (k=10) | O(k·N) edges, bounded per-user. At 100k: max 1M edges | Accepted (ADR-020) |
| Variable weights | Diversity metric bounds gaming regardless of weight values | Accepted (ADR-023) |
| Denouncer-only revocation | Per-user budget (d=2) limits coordinated attacks. 100k users → 200k max denouncements total, but each costs attacker a slot | Accepted (ADR-024) |
| One-hop cascade | Linear in edge count, bounded by slot limit | Accepted (ADR-024) |
| Time decay | Passive Sybil resistance without user action | Phase 3 (next) |

**Key scale insight:** The diversity metric is the core Sybil defense. A Sybil cluster can create arbitrarily many edges, but diversity counts *independent paths* through distinct community members. At 100k, the ratio of legitimate-to-Sybil paths makes it progressively harder for Sybils to achieve diversity ≥ 2. The slot budget (k=10) caps the number of paths any single user can contribute to the Sybil cluster's score. These two constraints compound: more Sybil nodes helps less than linearly because each new node must spend slots connecting to the legitimate graph, and those slots come at the cost of edges within the Sybil cluster.

---

## 5. What's Deferred Past 100k

| Mechanism | Why Deferred | Scale Trigger |
|---|---|---|
| Adjudication/slashing | Requires governance process design, quorum rules, evidence format | When trust status carries real-world consequences |
| Multi-hop propagation | Collateral damage increases with graph density | When graph density exceeds simulation coverage |
| Automated threshold cascade | Needs Sybil-resistant independence detection | When automated action is less risky than governance delay |
| Dynamic parameter tuning | k, d, decay rate adaptation | When network growth patterns are observed empirically |

These are not oversights — they are deliberate deferrals. Building adjudication without real governance experience would produce speculative process design. Building multi-hop propagation without density data would require tuning against synthetic assumptions. Phase 3 (time decay) is the right next step precisely because the temporal extension infrastructure is now in place and the mechanism is low-risk: decay doesn't require user action, it naturally prunes inactive relationships.

---

## 6. Roadmap

### Phase 3: Time Decay (Next — unblocked by PR #673)

Three candidate decay functions:
- **Exponential** (6-month half-life): smooth, natural-feeling decay; models fading memory of someone you met once
- **Step function** (full → half → zero over 2 years): discrete, predictable; users know exactly when their endorsement needs renewal
- **Linear**: simple to reason about; no abrupt drops

Simulation deliverables:
- Sybil attack window analysis: does a Sybil cluster's attack surface narrow naturally as fabricated edges decay?
- Stale-but-legitimate edge test: do real relationships survive without renewal under each decay model?
- Slot auto-release policy: what threshold triggers auto-release? Does auto-release create manipulation surface?

**Renewal = re-swap.** No new UX needed. The existing swap flow handles it — re-do the handshake, which overwrites the slot with new weight + fresh timestamp.

Deliverable: ADR-025 accepted with simulation evidence.

### Near-term (pre-launch)

- **Weight UI (#656):** Swap method + relationship depth selection in endorsement flow. Currently the weight is hardcoded; this exposes the ADR-023 table to users.
- **Trust dashboard polish:** Score card, eligibility messaging, remediation path clarity.
- **Seeded demo data:** Pre-populated trust graph for demo users so new visitors see a live, realistic network rather than an empty graph.

### Post-launch

- **Governance process design** — adjudication for severe cases (its own ADR, substantial design work).
- **Network monitoring** — detecting Sybil cluster formation in production data.
- **Cross-context trust** — how trust established in one community context informs trust in another.

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
