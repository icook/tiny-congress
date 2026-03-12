# Trust Graph Red/Blue Sybil Resistance Simulation

> **Status:** Design in progress. Blocked on finalizing trust handshake base abstractions.

**Issue:** #624
**Goal:** Validate the trust engine's Sybil resistance claims by simulating adversarial graph topologies against the real service layer.

---

## Decisions Made

- **Audience:** Developer/design tool (not user-facing, not demo artifact)
- **Primary scenario:** Sybil attack resistance — red team creates fake accounts in various topologies, observe whether they gain trust distance / path diversity / room eligibility
- **Where it runs:** Rust integration tests using the real service layer (`TrustEngine`, `TrustService`, constraints) against `isolated_db()` test databases
- **Approach:** Parameterized topology generators + named regression scenarios (Approach B from brainstorming)
- **Output:** Pass/fail assertions + structured summary table + DOT/Graphviz files with red/blue coloring and score annotations

## Architecture (Approved)

**Module location:** `service/tests/common/simulation/`

**Components:**
- `GraphBuilder` — programmatic topology construction (nodes with red/blue team designation, weighted directed edges, convenience methods like `add_clique`)
- `TopologyGenerator` — parameterized generators: `hub_and_spoke(hub, spoke_count)`, `chain(length)`, `colluding_ring(size)`, `healthy_web(size, density)`
- `SimulationReport` — runs `TrustEngine` from each anchor, collects scores, produces summary table + DOT file to `target/simulation/`

**Named scenarios (initial set TBD):**
- Hub-and-spoke Sybil attack
- Chain infiltration
- Colluding ring
- Mixed: healthy web + red cluster attached at single point
- (More to be defined after handshake abstractions solidify)

**Test pattern:** `#[shared_runtime_test]` + `isolated_db()` + direct `reputation__endorsements` inserts (same as existing `trust_engine_tests.rs`)

## Open Questions — Simulation

- **GraphBuilder node model:** Initial proposal used `enum Team { Blue, Red }` and `SimNode { id, name, team }`. Needs rethinking after handshake base abstractions are finalized — the simulation model should align with whatever the handshake establishes.
- **Topology generators:** Exact parameters and default values TBD.
- **Assertion invariants:** Need to define the specific properties we expect to hold (e.g., "red hub-and-spoke nodes never achieve `path_diversity >= 2`", "red chain of length > N exceeds distance cutoff"). These should be derived from the trust engine's actual algorithm properties.
- **Denouncement simulation:** Denouncements don't currently affect graph traversal. Should the simulation test what *would* happen if they did, or only test current behavior?

## Open Questions — Trust Architecture (from ADR audit, 2026-03-12)

Cross-reference: see `.plan/2026-03-12-sponsorship-risk-design.md` for sponsorship risk mechanism analysis.

### Blocking design gaps

- **Trust anchor bootstrap:** How does the first user (seed node) achieve `trust_distance = 0`? The recursive CTE (ADR-019) starts by finding users the anchor endorses — the anchor itself is never inserted into the result set. No ADR specifies the mechanism for populating the anchor's own score record. This is prerequisite for both simulation and production.
- **Denouncement model:** What does a denouncement *do* to the graph? Currently recorded but has no traversal effect (ADR-020). Must be modeled before sponsorship risk can be designed. Dependency chain: denouncement model → risk propagation → sponsorship cost.
- **Verifier slot exemption:** ADR-020 defines endorsement slots (k=3 demo) but has no carve-out for verifier/platform accounts (ADR-008). A bootstrap verifier would exhaust slots after 3 endorsements. Decision: platform endorsements should grant large/unlimited slots as a bootstrap mechanism. Needs to be formalized.

### ADR contradictions to resolve

- **`influence_staked` field (ADR-018 vs 020):** ADR-018 documents `influence_staked` as an active schema field. ADR-020 targets continuous influence for removal in favor of discrete slots. 018 should note the field as legacy.
- **"Real-time" room admission language (ADR-017 vs 021):** ADR-017 says room admission is "real-time." Under ADR-021's 24h batch, this means up to 24h post-action. Language in 017 needs clarification: "immediate against the latest snapshot" not "instant after handshake."
- **Room thresholds are room-configurable:** TRD and backend have different default thresholds (Community: distance ≤ 6.0/diversity ≥ 1 vs distance ≤ 5.0/diversity ≥ 2). Per ADR-017, these are room-level policy, not platform constants. ADR-017 should make this explicit.
- **Verifier endorsements under two-layer model (ADR-008 vs 017):** ADR-008 frames verifier endorsements as gating "voting eligibility" — a platform-level access decision. Under ADR-017's two-layer split, voting eligibility should be a room-layer decision. ADR-008 needs reframing.
- **`endorser_id` vs `issuer_id` naming (ADR-018 vs 008):** Same conceptual column, different names in documentation.
- **Sim worker vs batch model (ADR-009 vs 021):** Sim worker assumes real-time action effects; batch reconciliation invalidates this.
- **Partial-budget ordering (ADR-020 vs 021):** If a user submits 5 actions but budget is 3, which are applied? Ordering rule undefined.

### Missing cross-references

- ADR-021 → ADR-003 (pgmq queue infrastructure)
- ADR-018, 019, 017 → ADR-008 (verifier endorsements write same table)
- ADR-019 ↔ ADR-020 (score computation and slot allocation are mechanically coupled)
- **Status mismatch:** Accepted ADRs (017, 019) depend on Proposed ADRs (020, 021)

## Context

### Trust architecture (ADR series 017-021, PR #630)
- **ADR-017:** Two-layer split — platform trust (Sybil resistance) vs communication permission (rooms)
- **ADR-018:** Handshake protocol — Physical QR (1.0), Synchronous Remote (0.7), Social Referral (0.3), zero-PII
- **ADR-019:** Trust engine — recursive CTE, 1/weight cost, 10.0 cutoff, path diversity approximation
- **ADR-020:** Reputation scarcity — discrete slots (k=3/5), daily action budgets, sponsorship risk (principle only)
- **ADR-021:** Batch reconciliation — 24h action cadence, declared intentions processed at EOD

### Current backend state
- Weighted directed graph: edges in `reputation__endorsements`, cost = `1/weight`
- Influence budgets: 10.0 default, staked on endorsements, burned on denouncements (ADR-020 proposes replacing with slots)
- Denouncements stored but **do not affect traversal** yet
- Room constraints check distance + diversity thresholds (values are room-configurable per ADR-017)

### Existing test infrastructure
- `isolated_db()` for per-test DB isolation (PostgreSQL template copy)
- `AccountFactory::new().with_seed(N)` for deterministic account creation
- Direct endorsement insert helpers already used in `trust_engine_tests.rs`
- `TrustEngine::new(pool)` callable directly from tests

## Related Design Work

- `.plan/2026-03-12-sponsorship-risk-design.md` — sponsorship risk mechanisms (6 candidates evaluated, none selected), ADR audit details, verifier bootstrapping
- PR #630 (`docs/017-trust-architecture-adrs`) — the ADR series formalizing trust architecture decisions

## AI Tooling

Brainstormed with Claude Code (Opus 4.6). Design paused pending handshake abstraction work.
