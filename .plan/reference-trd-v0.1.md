# TRD v0.1 — Web-of-Trust Identity & Reputation Protocol

> **Reference copy.** Converted from `tiny-congress-trd.docx` (March 2026, DRAFT).
> Durable decisions from this document have been extracted into ADRs 017-021 (PR #630).
> This file is retained as reference for material not yet captured in ADRs:
> red team analysis, penalty system, test suite specification, and identity type taxonomy.
>
> **Superseded sections:** Sections 2 (handshake) and 3 (trust engine) are now
> covered by ADRs 018 and 019. Section 4 (scarcity) is covered by ADR-020.
> Section 7 (MVP scope) is partially covered by `objectives.md`.

---

## 1. Executive Summary

Tiny Congress is a structured democratic deliberation platform. Its core differentiator is a trust-threshold architecture that gates participation in deliberation rooms based on the verifiable trustworthiness of each participant's identity. This document formalizes the Web-of-Trust protocol that replaces centralized KYC with a social-graph-based trust model grounded in peer-to-peer handshakes, recursive reputation scoring, and finite endorsement budgets.

The system derives Sybil resistance not from biometric data or government-issued credentials, but from the topological properties of the trust graph itself: trust distance (depth from a known root), path diversity (independent routes through the graph), and endorsement scarcity (a finite budget of outbound trust signals).

## 2. Identity & Handshake Protocol

> **See ADR-018 for the current handshake specification.**

### 2.1 Identity Types

Every entity in the system resolves to one of three identity types. The critical invariant is: every vote must trace to exactly one Sovereign Human via a directed acyclic path in the trust graph.

| Identity Type | Definition | Trust Requirement |
|---|---|---|
| **Sovereign Human** | A unique biological individual. Claims "I am a real person." Root unit of accountability. | High path diversity + peer endorsements. Cannot be created programmatically. |
| **Delegated Agent** | A machine or bot acting on behalf of a Sovereign Human. Transparently parented. Voting weight drawn from parent's reputation. | Explicit parent-link to a Sovereign Human. Consumes one endorsement slot from the parent's budget. |
| **Group Proxy** | A collective identity (e.g., a chapter, union local, neighborhood association). Represents institutional rather than individual trust. | Multi-signature handshake from N founding members. Group trust power is finite and collectively governed. |

Design principle: Bots are welcome; deception is not. A Delegated Agent is a legitimate cognitive prosthetic. The violation occurs only when an entity claims Sovereign Human status without actually being one, thereby inflating the vote count.

### 2.2 Handshake Contexts

> **See ADR-018 for current specification.** The table below is the original TRD version.

| Context | Mechanism | Weight | PII Stored |
|---|---|---|---|
| Physical QR | JWT-signed QR, scanned in person | 1.0 | None |
| Synchronous Remote | Video call, attestation recorded | 0.7 | None |
| Social Referral | Invite link, auto-creates edge | 0.3 | None |

### 2.2.2 Handshake Schema (TRD version)

> **Note:** The TRD uses `voucher_id`/`vouchee_id` naming. The implementation uses `endorser_id`/`subject_id`. See ADR audit finding #8.

| Column | Type | Description |
|---|---|---|
| `vouch_id` | UUID | PK |
| `voucher_id` | UUID | FK → users.id, the identity extending trust |
| `vouchee_id` | UUID | FK → users.id, the identity receiving trust |
| `context` | ENUM | physical_qr, video_call, social_referral |
| `weight` | FLOAT | Trust weight based on context |
| `revoked_at` | TIMESTAMPTZ | Non-null if voucher revoked this edge |

## 3. The Trust Engine

> **See ADR-019 for the current computation specification.**

### 3.3 Composite Trust Score — Room Thresholds (TRD version)

> **Note:** ADR-017 makes thresholds room-configurable. These are reference values only.

| Room Tier | Distance Threshold | Diversity Threshold |
|---|---|---|
| Community | distance <= 6.0 | diversity >= 1 |
| Congress | distance <= 3.0 | diversity >= 2 |
| High Trust | distance <= 2.0 | diversity >= 3 |

## 4. Reputation Scarcity Model

> **See ADR-020 for current specification.** Key difference: TRD specifies stack-ranking of endorsements; ADR-020 uses simple slot occupancy (rejected stack-ranking as unnecessary complexity).

## 5. Red Team Analysis & Mitigations

> **NOT yet captured in ADRs.** This section is the primary reason for retaining the TRD as reference.

### 5.1 Attack Vectors

#### 5.1.1 Hub-and-Spoke Infiltration

**Scenario:** A single attacker creates N fake identities and endorses all of them personally, spending their entire endorsement budget on Sybil nodes.

**Graph signature:** A cluster with cut size 1. Removing the attacker disconnects all N nodes. All N nodes have path diversity = 1.

**Mitigation:** Path diversity thresholds for Congress and High Trust rooms (diversity >= 2, >= 3) structurally exclude these nodes. The attacker's fake identities can only access Community-tier rooms.

#### 5.1.2 Slow Infiltration (The Sleeping Giant)

**Scenario:** An adversary recruits 10 real humans who join honestly, participate for months, and earn High Trust. They then simultaneously spend their endorsement budgets on 50 sophisticated AI agents before a critical vote.

**Graph signature:** Sudden burst of new nodes with high trust distance scores in a short time window, all parented by a cluster of previously-quiet high-trust users.

**Mitigations:**
- **Trust Velocity Detection:** Monitor rate of new trust_edges per user per time window. Flag if a user creates k edges in 24 hours (budget dump).
- **Maturity Delay:** New identities' votes weighted at 0.1x for first 30 days.
- **Post-hoc Statistical Coherence:** If new agents vote with >95% correlation across rooms, flag as potential collusion cluster.

#### 5.1.3 Shadow Cluster (Coordinated Collusion)

**Scenario:** 10 real humans form a closed endorsement ring, verifying each other to create artificially high path diversity. They then introduce bots that appear well-connected.

**Graph signature:** Low external conductance. Dense internally, few edges to rest of graph. Like a tumor: organized mass that doesn't exchange trust with healthy tissue.

**Mitigation:** Compute ratio of internal to external edges using community detection (Louvain). Cluster with >80% internal edges flagged as suspicious. Diversity recalculated using only external paths.

#### 5.1.4 Graph Pruning (Topological Warfare)

**Scenario:** A faction coordinates to denounce key bridge nodes in the opposing faction's graph, increasing trust distance for hundreds of users.

**Graph signature:** Sudden increase in denouncements targeting high betweenness centrality nodes.

**Mitigations:**
- **Path Redundancy:** Users with diversity >= 3 immunized against single-point-of-failure slashing.
- **Denouncement Budget:** d=2 prevents mass-flagging campaigns.
- **Bridge Protection:** Nodes with betweenness centrality above 90th percentile require 3 independent denouncements from 3 different graph branches.

### 5.2 Structural Penalties

> **NOT implemented. Design reference only.** Penalty triggers depend on the denouncement model (open design question).

| Penalty | Trigger | Effect | Recovery |
|---|---|---|---|
| **Soft Slash** | Endorsed a flagged user. >90% healthy history. | Budget frozen 30 days. Voting retained. | Automatic after 30 days. |
| **Hard Slash** | Endorsed 3+ flagged users. Or >40% edges point to slashed nodes. | Distance set to infinity. All room access revoked. Budget = 0. | Restorative Handshake. |
| **Probation** | New account, or recovering from Soft Slash. | Votes weighted 0.5x. Cannot endorse. | 30 days clean + 1 new high-weight handshake. |
| **Quarantine** | Flagged as Sybil cluster member by statistical coherence. | Votes recorded but excluded from aggregates. User notified. | Jury Appeal. |

**Cascading Penalties:** When a node is Hard Slashed, each direct endorser receives +1.0 additive distance penalty. Not recursive beyond one hop — penalizes negligence, not guilt by remote association.

### 5.3 Restorative Justice

#### 5.3.1 The Restorative Handshake
A Hard Slashed user can restore reputation by completing a high-friction verification with a High Trust user from a different graph branch (minimum 3 hops from the slashed user's original endorsers).

#### 5.3.2 Jury Appeal
Quarantined user may appeal to 5 randomly selected jurors, each from different graph branches (minimum 3 hops apart). 3/5 majority restores; 4/5 majority restores full budget.

#### 5.3.3 The Whistleblower Incentive
If a user revokes an endorsement *before* the endorsed identity is flagged, the revoker receives no penalty and earns +1 endorsement budget. Rewards proactive self-policing.

## 6. Test Suite Specification

### 6.1 Graph Health Tests (SQL)

| Test Case | Setup | Assertion |
|---|---|---|
| Linear chain trust distance | Seed -> A -> B -> C, all physical_qr (1.0) | C.trust_distance = 3.0 |
| Mixed-weight distance | Seed -> A (1.0) -> B (social_referral, 0.3) | B.trust_distance = 1.0 + 3.33 = 4.33 |
| Path diversity: independent branches | Seed -> A -> X; Seed -> B -> X (A, B unrelated) | X.path_diversity = 2 |
| Path diversity: shared branch | Seed -> A -> B -> X; Seed -> A -> C -> X | X.path_diversity = 1 (both paths share A) |
| Revoked edge exclusion | Seed -> A -> B; A revokes B | B.trust_distance = infinity (unreachable) |
| Cycle prevention | Seed -> A -> B -> A (cycle) | No infinite loop. A.trust_distance from direct Seed path. |
| Hub-and-spoke detection | Attacker endorses 5 nodes. No other endorsers. | All 5 nodes have diversity = 1. Excluded from Congress. |

### 6.2 Handshake Flow Tests (Playwright)

| Test Case | Steps | Assertion |
|---|---|---|
| QR handshake happy path | A generates QR. B scans. Backend creates edge. | Edge exists with context = physical_qr, weight = 1.0 |
| Expired JWT rejection | A generates QR. Wait > TTL. B scans. | 401. No edge created. |
| Duplicate handshake prevention | A and B complete handshake. Attempt second. | 409 Conflict. Single edge in DB. |
| Room unlock on handshake | User at distance 3.0, diversity 1. Gets second handshake. | Score recalculates. Congress room appears. |

### 6.3 Sybil Swarm Simulation

Setup: 20 users with realistic topology. One attacker at distance 2.0.
Attack: Attacker creates 5 accounts via social_referral (0.3).

**Assertions:**
- All Sybil nodes have distance > 3.0 (Congress threshold)
- All Sybil nodes have diversity = 1
- Attacker's budget is fully spent (5/5 slots for k=5, or 3/3 for k=3 demo)

## 7. March 20th MVP Scope

> **See `objectives.md` for the current demo checklist.** Retained here for reference.

### 7.1 In Scope

- Sovereign Human identity only (Delegated Agent, Group Proxy deferred)
- Social Referral handshake (primary onboarding)
- Physical QR handshake (high-trust ritual)
- Trust Distance CTE, materialized
- Endorsement budget k=3
- Two-tier rooms: Community + Congress
- Trust tree visualization
- Revoke endorsement

### 7.2 Out of Scope

- Path Diversity (exact) — approximation sufficient
- Stripe Identity KYC — available as bolt-on
- Statistical coherence detection — needs real vote data
- Jury system — needs >50 users
- Delegated Agents / Bots — schema planned, no UI
- Group Proxy — post-demo
- Denouncement budget & slashing — graph too small
- Maturity delay — no value in 2-week demo window

## 8. Open Questions (TRD)

- Endorsement budget size calibration (k=3 demo, k=5 production)
- Trust distance threshold calibration
- Edge weight values (1.0 / 0.7 / 0.3 — are they right?)
- Liquid delegation UX for Delegated Agents
- Group Proxy trust power computation
- zkPassport / mDL integration (2026 Q3)
