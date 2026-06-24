# Trust Graph Red/Blue Sybil Resistance Simulation

> **Status:** Simulation harness shipped (PR #643). Diversity fix shipped (PR #652). Remaining work: denouncement experiments, sponsorship risk mechanisms.

**Issue:** #624
**Goal:** Validate the trust engine's Sybil resistance claims by simulating adversarial graph topologies against the real service layer.

### Progress (2026-03-12/13)
- **Simulation harness built and passing** — 6 named scenarios, all assertions green (PR #643)
- **Diversity approximation fixed** — replaced exploitable `COUNT(DISTINCT endorser_id)` with exact vertex connectivity via Edmonds-Karp max-flow (PR #652, fixes #648)
- **Trust anchor bootstrap resolved** — `compute_distances_from` now injects anchor at distance=0 (already on master)
- **Endorsement slots landed** — k=3 limit enforced with verifier exemption, `influence_staked` dropped (PR #640, migration 15)

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
- Coerced handshake: legitimate topology created under social pressure (tests whether mutual slashing mitigation works)
- Mercenary bot: well-integrated node that shifts voting behavior after trust accumulation (tests vote correlation detection)
- (More to be defined after handshake abstractions solidify)

**Test pattern:** `#[shared_runtime_test]` + `isolated_db()` + direct `reputation__endorsements` inserts (same as existing `trust_engine_tests.rs`)

## Empirical Findings (from simulation harness, PR #643)

| Scenario | Key result | Status |
|----------|-----------|--------|
| Hub-and-spoke Sybil | d=5.333, div=1 for all spokes | Structurally blocked |
| Chain infiltration | 2/8 red nodes unreachable (distance cutoff), all reachable have div=1 | Distance cutoff works |
| Colluding ring (6 nodes, single attachment) | div=1 for all ring nodes (post #652 fix) | Blocked by vertex connectivity |
| Red cluster (5 fully-connected, single attachment) | div=1 for all cluster nodes (post #652 fix) | Blocked by vertex connectivity |
| Social referral ceiling | Exactly 3 hops reachable at weight 0.3 (d=3.33/hop, cutoff at 10.0) | Distance limit works |
| Weight calibration | Physical=1.0, Video=1.43, Social=3.33 — target gets div=3 from 3 independent paths | Exact expected ratios |

## Open Questions — Simulation

- ~~**GraphBuilder node model:**~~ Resolved — `Team { Blue, Red }` + `SimNode { id, name, team }` works well. Shipped in PR #643.
- ~~**Topology generators:**~~ Resolved — `hub_and_spoke`, `chain`, `colluding_ring`, `healthy_web` with parameterized prefix/team/count/weight.
- ~~**Assertion invariants:**~~ Resolved — all 6 scenarios have specific numeric assertions validated against the engine.
- **Denouncement simulation:** Still open. Denouncements don't affect graph traversal. The harness is ready to test denouncement effects once the denouncement model is decided — add graph mutation helpers and parameterized denouncement layers on top of existing scenarios.

## Open Questions — Trust Architecture (from ADR audit, 2026-03-12)

Cross-reference: see `.plan/2026-03-12-sponsorship-risk-design.md` for sponsorship risk mechanism analysis.

### Blocking design gaps

- ~~**Trust anchor bootstrap:**~~ **RESOLVED** — `compute_distances_from` now injects anchor at distance=0 in the result set. Confirmed working in all 6 simulation scenarios.
- **Denouncement model:** What does a denouncement *do* to the graph? Currently recorded but has no traversal effect (ADR-020). Must be modeled before sponsorship risk can be designed. Dependency chain: denouncement model → risk propagation → sponsorship cost. **This is the primary remaining blocker.**
- ~~**Verifier slot exemption:**~~ **RESOLVED** — PR #640 implements verifier bypass of endorsement slot limits in `TrustService.endorse()`.

### ADR contradictions to resolve

- ~~**`influence_staked` field (ADR-018 vs 020):**~~ **RESOLVED** — migration 15 dropped `influence_staked` column. `update_influence` removed. Remaining cleanup (`trust__user_influence` table repurpose) tracked in #647.
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

## Design Heritage (from ~/tiny-congress-notes/)

### EigenTrust evaluation → CTE decision

The Nov 2024 research (`research/02_trust_reputation/`) extensively explored **EigenTrust** (PageRank-like iterative matrix-vector convergence) as the trust computation algorithm. EigenTrust properties: decentralized, convergent, manipulation-resistant by small clusters. Scaling estimate: convergence for 100M nodes with 100 edges each ≈ 25 minutes on a 1,000-node distributed cluster (50 iterations × ~30s/iteration).

**Why CTE won for MVP:** Simpler, runs directly in Postgres, sufficient for <100 users. No distributed infrastructure required. The recursive CTE (ADR-019) with `cost = 1/weight` produces trust distance — a single scalar per user pair. EigenTrust produces a global ranking (more like PageRank centrality).

**Future upgrade path:** As the network grows past ~1,000 users, the CTE approach may hit performance limits. EigenTrust or similar iterative convergence should be evaluated as a replacement. The batch reconciliation model (ADR-021) is architecturally compatible with either approach — both produce materialized snapshots.

### Multidimensional trust (deliberate simplification)

The research describes trust as **5 independent dimensions**: Identity, Intentional, Reliability, Competence, Proximity — each with its own decay rate and weight. The current implementation uses a **single scalar** (trust distance from CTE).

**This is a deliberate simplification for MVP, not a design rejection.** Multiple dimensions remain relevant as room-level enrichment:
- **Handshake context** (physical QR vs video vs social referral) — already captured as edge weight
- **Claims verified** (which verifiers have attested what)
- **Length of relationship** (edge age, renewal history)
- **Endorser reputation** (trust-flows-downhill dynamic)

Rooms can factor any of these into their constraint models via ADR-017's `constraint_config` JSONB. Platform eligibility uses only trust distance + path diversity. Room-level trust modeling is richer and intentionally room-configurable.

### Bounded weight multiplier (room-level, not platform)

The Aug 2025 whitepaper (`historical/2025-08-25 whitepaper.md`) establishes a "soft gate" mechanism: trust scores modulate voting weight between **0.5x and 3.0x**, never silencing eligible users. This is distinct from room eligibility (the "hard gate").

**Decision:** This is a room-level capability, not a platform concern. A room can choose to weight votes by trust score within the 0.5x–3.0x bounds. Platform eligibility is binary (meets threshold or doesn't). The multiplier is not implemented for March 20 — all eligible participants vote at 1.0x.

### Additional attack vectors (from Gemini brainstorm, 2026-03-05)

Two attack vectors from `~/tiny-congress-notes/03-05-2026-gemini.md` not covered in the TRD red team analysis:

**Coerced Handshake (Boss Extortion):** Authority figure pressures subordinates into QR handshakes, creating real-but-involuntary trust edges. Graph signature: legitimate topology (real humans, real handshakes), but the social pressure behind the edges is coercive. Proposed mitigation: mutual slashing — if an endorsee is flagged, the endorser loses ALL endorsements (not just the one), making coercion too costly because the coercer risks their entire graph position.

**Mercenary Bot (Pro-Social Trojan):** A helpful bot accumulates endorsements over months of legitimate participation, then silently shifts voting behavior before a critical vote. Graph signature: well-integrated node with sudden behavioral change (detectable only via vote correlation analysis, not graph topology). Proposed mitigation: strict human/bot vote separation — human votes and delegated agent votes are always separately visible and togglable in room result aggregation. This makes behavioral shifts detectable by separating the channels.

Both scenarios should be modeled as named simulation scenarios once the denouncement model is resolved.

## Context

### Trust architecture (ADR series 017-021, PR #630)
- **ADR-017:** Two-layer split — platform trust (Sybil resistance) vs communication permission (rooms)
- **ADR-018:** Handshake protocol — Physical QR (1.0), Synchronous Remote (0.7), Social Referral (0.3), zero-PII
- **ADR-019:** Trust engine — recursive CTE, 1/weight cost, 10.0 cutoff, path diversity approximation
- **ADR-020:** Reputation scarcity — discrete slots (k=3/5), daily action budgets, sponsorship risk (principle only)
- **ADR-021:** Batch reconciliation — 24h action cadence, declared intentions processed at EOD

### Current backend state (as of 2026-03-13)
- Weighted directed graph: edges in `reputation__endorsements`, cost = `1/weight`
- **Endorsement slots** (k=3 demo) — replaced continuous influence. Verifiers exempt. (#640)
- **Vertex connectivity** — exact node-disjoint path count via Edmonds-Karp max-flow, replacing the old `COUNT(DISTINCT endorser_id)` approximation (#652)
- **Trust anchor bootstrap** — anchor injected at distance=0 by `compute_distances_from`
- **Topic filter** — distance CTE and diversity edge loading now filter `topic = 'trust'` (#652)
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
- PR #640 (`refactor/636-endorsement-slots`) — endorsement slots implementation (k=3, verifier exemption, influence_staked dropped)
- PR #643 (`test/624-trust-simulation-harness`) — simulation harness with 6 named scenarios, all passing
- PR #652 (`fix/648-diversity-vertex-connectivity`) — replaced diversity approximation with exact vertex connectivity
- #647 — remaining cleanup: `trust__user_influence` table repurpose
- `~/tiny-congress-notes/historical/adr-component-boundary-contracts.md` (Feb 2026) — three-component modular monolith ADR (Identity / Reputation / Rooms) with `tc_trust::TrustResolver` interface. Has "sort of landed" in the codebase but needs a gap analysis and formal ADR. Defines the `resolve(actor_id, room_trust_policy, identity_state, endorsement_state) → EffectiveContext` contract and "sync, don't query" principle (Rooms cache trust state via events, not live queries). Simulation should model cached reads, not RPC.
- `~/tiny-congress-notes/research/02_trust_reputation/` — EigenTrust evaluation, multidimensional trust model, endorsement mechanics research
- `~/tiny-congress-notes/03-05-2026-gemini.md` — the original Gemini brainstorm that produced the trust architecture. Source for coerced handshake and mercenary bot attack vectors.

## AI Tooling

Brainstormed with Claude Code (Opus 4.6).
