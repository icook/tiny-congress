# Milestones to Public Beta

**Date:** 2026-03-19
**Status:** Active roadmap
**Context:** Progression from current demo state to public beta with real users. Each milestone has clear requirements and a "done when" definition.

---

## Current State (March 19, 2026)

- App live at tinycongress.com with demo verifier (fake identity verification)
- One room type (polling) with brand ethics content seeded by LLM
- Trust system fully designed and simulation-validated (4 ADRs accepted)
- Endorsement/denouncement backend and UI built
- Self-hosted CI/CD, observability, GitOps deployment
- A few close friends have seen it; no real external users yet

---

## Milestone 1: Invite-Capable

**Theme:** Anyone I endorse can participate. No fake verifier.

**Requirements:**
- [ ] #754 — Endorsement split: out-of-slot endorsements allowed, explicit slot selection
- [ ] #755 — Room capability tiers: Owner + Participant, `endorsed_by(owner)` gate
- [ ] #756 — Demo verifier disabled; rooms gated on endorsement, not fake verification
- [ ] Signup flow works without verification redirect
- [ ] User sees clear explanation of what they can do and why (tier transparency UX)

**Done when:** I can send someone a link, they sign up, I endorse them (QR or remote), and they can enter the brand ethics room and vote. No one explains anything to them — the UI is self-explanatory.

**Scope:** ~1000-1400 LOC across 3 tickets. Estimated: 1-2 weeks.

**Unlocks:** Real user testing with hand-picked participants. Every subsequent feature gets real feedback.

---

## Milestone 2: Engaging Room

**Theme:** The brand ethics room is worth inviting someone to.

**Requirements:**
- [ ] #757 — Steerable research: participants submit investigation suggestions, LLM agent generates evidence from them
- [ ] #758 — Research report synthesis: shareable artifact aggregating evidence + votes + community assessment
- [ ] Evidence cards show provenance (which suggestion, which search, when)
- [ ] Research feed updates visibly when new evidence arrives
- [ ] Report has a shareable public URL

**Done when:** A participant can suggest "look into Nike's labor practices," see new evidence cards appear within an hour, and share a synthesized report link that makes sense to someone who hasn't used the platform.

**Scope:** ~1400-1800 LOC across 2 tickets. Estimated: 2-3 weeks.

**Unlocks:** Word-of-mouth sharing. The report is the viral artifact — it gives non-users a reason to care.

---

## Milestone 3: Multi-Room

**Theme:** More than one interesting thing to do on the platform.

**Requirements:**
- [ ] Pairwise ranking room type (Bradley-Terry head-to-head, stack ranking output)
- [ ] Room creation by Owner (pick a module, configure via template)
- [ ] Room directory / discovery (beyond sidebar accordion)
- [ ] Per-room operations bot (sim → room-level plugin, the architecture fix noted in design workspace)
- [ ] At least 3 rooms with distinct topics demonstrating different interaction models
- [ ] **Start LLC registration** (#765) — 2-6 week lead time before it's needed in Milestone 5

**Done when:** A new user sees a directory of rooms, each with a different interaction model, and can participate in whichever interests them. Room owners can create new rooms from templates without code changes. LLC paperwork is filed.

**Scope:** Large. Estimated: 3-5 weeks. Requires extracting the module interface from polling code (design exists in `.plan/2026-03-17-room-types-architecture.md`).

**Unlocks:** Platform feels like a platform, not a single-room demo.

---

## Milestone 4: Trust at Scale

**Theme:** Trust system works for real social graphs, not just founder-endorsed circles.

**Requirements:**
- [ ] #685 — Sparse Edmonds-Karp in engine (currently dense O(n²) matrix)
- [ ] #686 — Time decay integration (ADR-025 step function)
- [ ] Multi-hop endorsement paths work reliably (users endorse users who endorse users)
- [ ] Trust dashboard shows meaningful information for non-founder-adjacent users
- [ ] Graph health monitoring (#689)
- [ ] Endorsement slot management UX is intuitive (phase 2 of #754)

**Done when:** 50+ users with organic endorsement graph (not all directly endorsed by founder). Trust scores are meaningful, explainable, and the dashboard helps users understand their position.

**Scope:** Medium. Mostly engine work + UX polish. Estimated: 2-3 weeks.

**Unlocks:** Growth beyond founder's personal network.

---

## Milestone 5: Public Beta

**Theme:** A stranger can find this, sign up, and have a good experience.

**Requirements:**
- [ ] #765 — LLC registration complete (entity, EIN, bank account)
- [ ] Real identity verification (ID.me or equivalent — requires LLC for provider contract)
- [ ] Landing page that explains the product in one sentence
- [ ] Onboarding flow that doesn't require any explanation
- [ ] Cost tracking visible in UI (OpenRouter spend transparency)
- [ ] Donation mechanism (drives sustainability — requires LLC for payment processing)
- [ ] Terms of Service and Privacy Policy (requires LLC)
- [ ] Moderation tools (content flagging, at minimum)
- [ ] Mobile Safari fully tested and reliable
- [ ] Performance: <3s initial load, <1s interactions
- [ ] Basic abuse detection / rate limiting beyond current levels
- [ ] 25+ active users with meaningful engagement

**Done when:** I can post the link publicly (HN, civic tech communities, friends of friends) and people can use it without any personal onboarding from me.

**Scope:** Large. Estimated: 4-8 weeks. The identity verification integration is the longest pole.

**Unlocks:** Real public usage. Soliciting donations. Growth.

---

## Summary

| Milestone | Theme | Key Deliverable | Est. Weeks |
|-----------|-------|-----------------|------------|
| 1. Invite-Capable | Real access control | Endorsement = invite | 1-2 |
| 2. Engaging Room | Worth inviting to | Steerable research + shareable reports | 2-3 |
| 3. Multi-Room | Platform depth | Room templates + pairwise ranking | 3-5 |
| 4. Trust at Scale | Organic growth | Sparse engine + multi-hop trust | 2-3 |
| 5. Public Beta | Strangers welcome | ID verification + onboarding + donations | 4-8 |

**Total estimated: 12-21 weeks to public beta** (solo dev pace, including the unexpected).

Milestones 1-2 are the critical path — they determine whether anyone besides the founder finds value in the platform. Milestone 3 is where the "composable rooms" vision becomes real. Milestone 4 is when the trust system proves itself with real social dynamics. Milestone 5 is the launch gate.

---

## GitHub Milestone Reconciliation

The old milestones were workstream-oriented (horizontal concerns). The new milestones are product-stage-oriented (vertical gates). Each ticket belongs to the product milestone it gates.

| Old Milestone | Disposition |
|---|---|
| Demo: Friends & Family (Mar 20) | **Closed.** Open items rolled into M1: Invite-Capable |
| Growth: Scale Hardening | **Merged into M4.** Tickets moved to M4: Trust at Scale |
| 1-5 (Foundation, DX, Test, CI, Architecture) | **Open but dormant.** Remaining items can be reassigned to product milestones as they become relevant. Labels (`area/backend`, `type/feature`, etc.) capture the workstream dimension. |

**LLC timeline:** Registration starts in M3, completes before M5. Three M5 requirements depend on it: identity verification provider contract, donation processing, and legal documents (ToS/PP).

---

## What's NOT on this roadmap (post-beta)

- Elected roles / governance within rooms
- Federation (external room services)
- Selective disclosure (public vs private responses)
- Multi-anchor trust (replacing founder as sole root)
- Sybil detection heuristics beyond graph structure
- Account compromise detection and response
- Adjudication / slashing governance process
