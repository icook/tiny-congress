# Design State & Open Questions — Post-Brainstorm Synthesis

> **Reference copy.** Converted from `tiny-congress-design-state.docx` (March 10, 2026).
> This document consolidates the Gemini brainstorming session, Claude TRD, and implementation planning.
> Durable decisions have been extracted into ADRs 017-021 (PR #630).
> This file is retained for material not yet captured: falsification criteria,
> success/failure definitions, risk register, and calibration questions.
>
> **Superseded sections:** Section 2.2 (architecture summary) and 2.3 (decisions)
> are now covered by ADRs 017-021. Section 4 (implementation plan) is outdated —
> the 7-loop structure was written pre-ADR and pre-M3/M4 milestones.

---

## 1. Where We Are

### 1.1 Current State of the Codebase (as of March 10)

- **Working:** Auth, registration, basic room abstraction, multi-dimensional polling with slider inputs, result visualization, CD pipeline.
- **Sloppy:** ~3-5k lines of unreviewed Claude-generated code. CI passes but code quality not manually validated.
- **Missing:** The trust/identity layer is trivial. No web-of-trust, no endorsement budgets, no tiered room access.

> **Update (March 12):** Trust engine backend shipped (PR #555, ~6,000 lines). Frontend trust UI is M3 work (feature/612-m3-trust-ui). QR handshake is M4.

### 1.2 What Changed During This Design Session

The project moved from centralized KYC to a Web-of-Trust identity system. This produced:
- A Technical Requirements Document (TRD)
- Seven implementation loop prompts
- A CLAUDE.md addendum
- An implementation strategy: clean sessions per loop, git worktrees, review-then-comment

## 2. The Design as Proposed

### 2.1 Philosophical Foundation

The central insight: in a structured deliberation platform where user outputs are slider positions and votes, traditional "bad behavior" (spam, slurs, disruption) is structurally impossible. The only meaningful violation is identity fraud.

This shifts the security model from **behavioral moderation** (what people say) to **topological validation** (how identities exist in the trust graph). A "bot" is not detected by what it does but by how it was created and who vouches for it.

Bots are not inherently bad. The system should welcome Delegated Agents as cognitive prosthetics. The violation is deception, not automation. Every vote must trace to exactly one Sovereign Human.

### 2.2 Architecture Summary

> **See ADRs 017-021 for current architecture.**

Four layers: Identity, Handshake, Trust Engine, Scarcity.

### 2.3 What's Decided

> **See ADRs 017-021.** The table below is the original design state version.

| Decision | Rationale |
|---|---|
| Web-of-Trust over centralized KYC | Philosophically aligned, avoids B2B sales cycles, PII storage liability |
| Trust Distance via weight-inverted CTE | Edge cost = 1.0/weight. High-friction verification dramatically more valuable |
| Finite endorsement budget (k=3 demo) | Caps total trust at N*k. Makes endorsements deliberate |
| Two-tier rooms (Community + Congress) | Demonstrates trust-gated access |
| Zero-PII invariant | Stores attestations, never documents |
| Directional trust edges | A endorsing B != B endorsing A |
| Clean sessions per implementation loop | No sub-agent chaining |
| Review-and-comment discipline | Correctness now, style later |

## 3. Open Questions

### 3.1 Trust Engine Calibration

#### 3.1.1 Should room thresholds be self-adjusting?

**Urgency:** Medium. Not needed for 15-person demo.

Three alternatives:
- **Percentile-based:** Top 60th percentile diversity. Self-adjusting but not legible.
- **Graph-diameter anchored:** Thresholds as multipliers of median metrics. Self-correcting.
- **Elected thresholds:** Users vote on the bar. Risk: incumbents "pull up the ladder."

**Proposed resolution:** Ship hardcoded for demo. Instrument distributions. Switch to percentile-based after 50+ users.

> **ADR-017 update:** Thresholds are room-configurable. Self-adjustment is a room-level decision, not platform-level.

#### 3.1.2 Are the edge weights right?

**Urgency:** Low. Tunable post-demo.

Physical QR = 1.0, video call = 0.7, social referral = 0.3. The 3.33x distance penalty for social referral may be too harsh or too lenient.

**Open sub-question:** Should edge weight incorporate the endorser's trust score? Creates "trust flows downhill" dynamic. Mathematically elegant but complicates CTE and introduces feedback loops.

#### 3.1.3 Path diversity approximation vs. exact computation

**Urgency:** Low. Approximation fine for <100 users.

> **ADR-019:** Decided to use approximation. Exact computation (max-flow) rejected for MVP.

### 3.2 Identity & Agency

#### 3.2.1 Delegated Agents: when and how?

**Urgency:** Low. Post-demo. Schema should not preclude it.

Open questions: How does a human "program" a bot's voting behavior? Rules-based? LLM-powered? Manual delegation? Does the bot consume a persistent endorsement slot?

**Proposed resolution:** Add `identity_type` column (defaulting to `sovereign_human`). No delegation UI until post-demo.

#### 3.2.2 Group Proxy trust power

**Urgency:** Low. Post-demo entirely.

How is group trust power computed? Sum of members? Fixed budget? Voted allocation? Can groups endorse individuals?

**Proposed resolution:** Defer entirely.

### 3.3 Scarcity & Penalties

#### 3.3.1 Endorsement budget size

**Urgency:** High. Must be decided before implementation.

k=3 in a 15-person network means each user can endorse 20% of the network. Too generous? Too restrictive?

**Proposed resolution:** Start k=3. Adjust based on demo feedback. Make it a config constant.

> **ADR-020:** k=3 demo, k=5 production. Decision made.

#### 3.3.2 How do violations manifest?

**Urgency:** Medium.

Three concrete violation types:
- **Collusion (Human Botnet):** Coordinated bloc voting identically. How correlated is "too correlated"?
- **Handshake Farming (Negligence):** Endorsing strangers to level up. Detectable only when endorsee is later flagged.
- **Identity Split (Shadow Accounts):** One human, two accounts. Same graph signature as a new user who only knows one person.

**Proposed resolution:** For demo, manual flagging with 3-flag threshold. Automated detection is post-demo research.

#### 3.3.3 Malicious slashing and faction warfare

**Urgency:** Low. Not a real threat at 15 users.

**Proposed resolution:** Defer. Admin restore is the escape hatch. Build graph simulation before implementing automated defenses.

### 3.4 Validation Strategy

#### 3.4.1 Graph simulation: ABM vs. LLM personas

**Urgency:** Medium. Post-demo, pre-public launch.

Three approaches in order of rigor:
1. **Graph-theoretic analysis:** Synthetic graphs with known topology. Run CTE. Validate thresholds. Cheapest, most reproducible.
2. **Agent-based modeling (ABM):** 5-6 behavioral archetypes. Monte Carlo sweeps. Tests structural mechanics.
3. **LLM persona simulation:** Synthetic agents with persona prompts. Validity problems: LLMs are more cooperative than real humans.

**Proposed resolution:** Start with graph-theoretic (weekend project). Then ABM. Save LLM personas for deliberation UX testing.

> **PR #625 update:** Graph simulation design is in progress. See `.plan/2026-03-12-trust-graph-simulation-design.md`.

#### 3.4.2 Falsification criteria

> **NOT captured elsewhere. This is critical reference material.**

**Urgency:** High. Should be written before demo ships.

What would make us abandon the web-of-trust approach?

- If >50% of demo users say the endorsement budget is confusing and don't understand why they can't invite everyone, the scarcity model needs fundamental rethinking.
- If the trust tree visualization produces zero "aha" reactions, the graph-based model may not be legible enough to differentiate the product.
- If the QR handshake feels like a chore (most users skip it or complain), high-friction verification is wrong for this audience.
- If users cannot explain what their "trust score" means after 10 minutes, the abstraction is too complex for a consumer product.

## 4. Implementation Plan Summary

> **Outdated.** The 7-loop structure was written pre-ADR. Current execution is tracked via GitHub issues and milestone "Demo: Friends & Family (Mar 20)". Retained for historical context only.

## 5. Risk Register

| Risk | Impact | Likelihood | Mitigation |
|---|---|---|---|
| CTE bug produces wrong trust scores silently | All room access wrong. Trust model invalidated. | High | Manual SQL validation. Regression tests. Review CTE for correctness. |
| Endorsement budget feels like barrier, not feature | Users frustrated. Trust model = friction. | Medium | k is a config constant. Adjust after first 5 users. UI must frame scarcity as exclusive. |
| QR handshake broken on mobile Safari/Chrome | Core demo flow fails on actual devices. | Medium | Manual QA on real phones. Fallback manual entry. |
| Trust tree visualization confusing rather than illuminating | "Aha moment" becomes "what am I looking at." | Medium | Keep simple. Force-directed graph. Mobile fallback to list. Test with one person first. |
| Code quality degrades under velocity | Codebase becomes unmaintainable. | Medium | Review-and-comment discipline. TODO(janitor). Dedicated cleanup sessions. |
| Infrastructure avoidance pattern reasserts | Trust system = procrastination. Demo slips. | Low | Falsification criteria. Time-box implementation. Ship without trust if not done. |

## 6. What Success Looks Like

### 6.1 March 20 Demo Success

A friend receives an invite link. They register, see their trust score (distance 3.33, diversity 1), and enter a Community room. They meet Isaac in person, scan a QR code, trust score improves. "Congress room unlocked." They enter Congress room, vote on a local issue. Open the trust tree, see their position. Send their invites to friends.

The experience should feel **intentional and exclusive** — not like signing up for another app, but like joining a community that takes trust seriously. Scarcity should feel like a feature. The QR handshake should feel like a ritual.

### 6.2 Post-Demo Success

- At least 10 people complete the full flow (invite -> community -> handshake -> congress)
- At least 3 people send their own invites (graph grows organically)
- Feedback includes specific, actionable criticism of the trust model
- Trust tree produces at least one "oh, that's interesting" reaction
- Isaac has clear signal on whether to double down, pivot, or layer on KYC

### 6.3 What Failure Looks Like

- Nobody completes the full flow because trust model is too confusing
- People register but never do QR handshake (friction outweighs perceived value)
- Endorsement budget creates frustration without exclusivity
- Isaac has same amount of product-market fit information after demo as before
