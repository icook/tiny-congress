# Sponsorship Risk & Denouncement Model — Design Brief

**Date:** 2026-03-12
**Branch:** test/624-trust-graph-simulation
**Status:** Exploratory — no implementation decisions made yet. Blocked on denouncement model.
**Related:** ADR-020 (reputation scarcity), GitHub #624 (trust graph simulation)
**Updated:** 2026-03-13 — factored in slots landing (#640), diversity fix (#652), anchor bootstrap resolution

## Problem Statement

ADR-020 establishes the *principle* that sponsors bear risk for endorsees, but explicitly defers the mechanism. Before sponsorship risk can be designed, we need a denouncement model — risk is triggered by "someone you endorsed gets denounced," and denouncements currently don't affect the trust graph (ADR-020: "recorded but do not currently affect trust graph traversal").

**Dependency chain:** denouncement model → risk propagation → sponsorship cost

## Design Context

### Architecture (from ADRs 017-021)

- **Discrete endorsement slots** (k=3 demo, k=5 prod) — the scarcity currency. **Landed** in PR #640 with verifier exemption.
- **Daily action budgets** — renewable, use-it-or-lose-it rate limit
- **24h batch reconciliation** — actions are declared intentions, reconciled at EOD
- **Trust distance** — recursive CTE, 1/weight cost, 10.0 cutoff. Now filters `topic = 'trust'` (#652).
- **Vertex connectivity** — exact node-disjoint path count via Edmonds-Karp max-flow (#652). Replaced the old `COUNT(DISTINCT endorser_id)` approximation which was exploitable by dense clusters.
- **Two-layer split** — platform trust (Sybil resistance) vs room permission (independent gating)
- **Denouncement budget** — d=2 per user, permanent (non-refundable)

### User's stated goals

- Sponsors need "skin in the game" for people they invite
- Risk should decrease as endorsee gains diverse handshakes (independent validation)
- Multiple sponsors may be required for full activation
- System should be slow-moving — visibility of coordinated attacks over time

## Open Question: Denouncement Model (Prerequisite)

Before sponsorship risk, we need to decide what a denouncement *does* to the graph. Options not yet evaluated:

- Does a denouncement increase the target's trust distance?
- Does it reduce their diversity count?
- Does it sever the edge entirely (equivalent to forced revocation)?
- Does it flag the target for manual review without graph effect?
- Do multiple denouncements from independent sources have compounding effect?
- How does batch reconciliation interact with denouncement processing?

## Sponsorship Risk Mechanisms Considered

Six mechanisms were brainstormed. None selected — captured here for future sessions.

### A. Probation slot — endorsement costs double until endorsee diversifies

New endorsement consumes **2 slots** instead of 1 (probation + normal). Probation slot released when endorsee reaches diversity threshold (e.g., 2+ independent endorsers).

- **Pro:** Very legible ("1 slot locked until Alice gets another endorser")
- **Pro:** Simple to implement — count probationary vs resolved endorsements
- **Con:** Possibly too punishing at k=3 (endorse one person → only 1 slot left)
- **Con:** Doesn't distinguish bad endorsements from new ones

### B. Cascading distance penalty — endorsee denouncement infects sponsor

Endorsee denounced → sponsor's trust distance increases by `penalty / endorsee_diversity_at_denouncement_time`.

- **Pro:** Risk truly decreases with endorsee diversification
- **Pro:** Fits the CTE model mathematically
- **Con:** Hard to explain to users ("your distance went from 2.1 to 3.4")
- **Con:** Requires denouncement to affect the graph (currently doesn't)

### C. Activation gate — endorsement pending until co-sponsored

Endorsement doesn't become a traversable edge until endorsee has N independent endorsers (N=2 for demo). Until then, "pending endorsement" — visible but not in graph computation.

- **Pro:** Strongest Sybil resistance — structurally prevents hub-and-spoke attacks
- **Pro:** Aligns with "multiple sponsors may be required"
- **Con:** Chicken-and-egg for bootstrapping (solved by trust anchor exemption)
- **Con:** Needs clear "pending" UX

### D. Slot forfeiture — denouncement burns sponsor's slot

Endorsee denounced → sponsor's slot permanently burned (or locked for N cycles). k=3 becomes k=2.

- **Pro:** Very high stakes, creates strong caution
- **Pro:** Simple to understand
- **Con:** Can be weaponized (denounce someone to hurt their sponsor)
- **Con:** With d=2, attack is expensive but possible

### E. Endorsement decay — edges weaken without renewal

Endorsements lose weight over time (e.g., -20% per cycle). Renewal costs a daily action to maintain full weight.

- **Pro:** Natural graph pruning — abandoned edges fade
- **Pro:** Action budget becomes scarce resource for maintenance
- **Pro:** Fits "slow, intentional" theme
- **Con:** Potentially annoying UX ("renew 3 endorsements today")
- **Con:** May discourage endorsing at all

### F. Graduated activation — endorsements strengthen over time

New endorsements start at reduced weight (e.g., 0.3× context weight), increase toward full weight over N cycles *if* endorsee diversifies.

- **Pro:** Smooth, no cliff edges
- **Pro:** Risk decreases as endorsee proves themselves
- **Pro:** Works without denouncement — proactive, not reactive
- **Con:** Harder to explain ("your endorsement is at 60% strength")

### Preliminary synthesis (not decided)

Combining **C (activation gate) + F (graduated activation)** was suggested as a two-layer approach mirroring the trust architecture: binary gate (activated?) + continuous signal (edge strength). Trust anchor exemption solves C's bootstrap problem. Not endorsed by user — needs further discussion.

## Verifier Bootstrapping Problem

~~ADR audit revealed ADR-008 (account-based verifiers) conflicts with the new slot system. Verifier accounts would exhaust k=3 slots after 3 endorsements.~~ **Slot exemption resolved** — PR #640 implements verifier bypass in `TrustService.endorse()`.

**Remaining open question:** How to incentivize users to graduate from platform-only trust to peer-verified trust. Possible: platform endorsements carry low weight (like social referral at 0.3), so platform-bootstrapped users have high trust distance and are incentivized to get real handshakes. The simulation harness can test this by modeling verifier endorsements at 0.3 weight and measuring resulting trust distance for verifier-only vs peer-endorsed users.

## ADR Audit Findings (2026-03-12)

Full audit of ADRs 017-021 against 001-016 identified these issues:

### Breaking contradictions
1. ~~**018 vs 020:** `influence_staked` documented as active (018) but targeted for removal (020)~~ **RESOLVED** — migration 15 dropped the column. Remaining table repurpose in #647.
2. ~~**020 vs 008:** No slot exemption for verifier accounts~~ **RESOLVED** — PR #640 implements verifier bypass.
3. **021 vs 009:** Sim worker assumes real-time effects; batch model breaks this

### Architectural tensions
4. **017 vs 008:** "All humans welcome" vs verifier endorsements gating voting eligibility
5. ~~**019 vs 018:** Trust anchor's own distance=0 never populated by CTE~~ **RESOLVED** — anchor injected at distance=0 by `compute_distances_from`.
6. **021 vs 017:** "Real-time" room admission language misleading under 24h batch
7. **020 vs 021:** Partial-budget ordering rule undefined (5 actions submitted, budget is 3)
8. **018 vs 008:** `endorser_id` vs `issuer_id` naming divergence

### Missing cross-references
- 021 → 003 (pgmq), 018/019/017 → 008 (verifiers), 019 ↔ 020 (coupled systems)
- Status mismatch: Accepted ADRs (017, 019) depend on Proposed ADRs (020, 021)

## Additional Attack Vectors (from Gemini brainstorm)

Two scenarios from `~/tiny-congress-notes/03-05-2026-gemini.md` not covered in the TRD red team (§5.1) and relevant to sponsorship risk design:

### Coerced Handshake (Boss Extortion)

Authority figure pressures subordinates into QR handshakes. The resulting trust edges are topologically legitimate (real humans, real handshakes) but socially coerced. This is invisible to graph analysis — the topology looks healthy.

**Implication for sponsorship risk:** Mutual slashing (endorser loses all endorsements if endorsee is flagged) was proposed as mitigation. This makes coercion costly for the coercer — they risk their entire graph position. However, it also makes *any* endorsement high-stakes, which interacts with mechanisms A-F above. If combined with D (slot forfeiture), the penalty for a coerced-then-flagged endorsee could be devastating.

### Mercenary Bot (Pro-Social Trojan)

A bot participates helpfully for months, accumulates endorsements from genuine users, then shifts voting behavior before a critical vote. Undetectable by graph topology — the node is well-integrated.

**Implication for sponsorship risk:** This attack bypasses all pre-endorsement risk mechanisms (A, C, F). The endorsement was genuinely earned. Only post-hoc detection (vote correlation analysis, behavioral anomaly flagging) can catch it. Mitigation: strict human/bot vote separation in room aggregation — human and delegated agent votes always shown separately and togglable. Sponsors of a mercenary bot should face lighter penalties (soft slash) since due diligence was reasonable.

## Next Steps (suggested, not committed)

1. **Model denouncements** — decide what a denouncement does to the graph before designing risk propagation. **This is the critical blocker.** The simulation harness (PR #643) is ready to test denouncement effects once the model is decided.
2. ~~**Resolve ADR-008 conflicts** — reframe verifier endorsements under two-layer architecture, add slot exemption~~ **Partially resolved** — slot exemption landed (#640). ADR-008 reframing under two-layer model still needed.
3. **Fix ADR cross-references** — quick cleanup pass on 017-021
4. **Then return to sponsorship risk** — with denouncement model in hand, evaluate mechanisms A-F against simulation scenarios
5. ~~**Red/blue simulation** (#624)~~ **DONE** — PR #643 ships the harness with 6 passing scenarios. Add denouncement experiments as parameterized layers on top of existing scenarios.
