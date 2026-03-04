# TinyCongress Demo Readiness Checklist

> **Target:** Friends & family demo by March 20, 2026
>
> **Core flow:** Signup → Verify identity → Enter room → Vote on multi-dimensional poll → See results

---

## Must Have — The Demo Breaks Without These

These are the things where, if missing, someone gets stuck or confused and closes the tab.

### Onboarding & Identity

- [ ] **Landing page that explains what this is in one sentence.** Not your whitepaper — one line like "TinyCongress lets verified people vote on issues that matter, with more nuance than yes/no." Non-technical people need this before they'll create an account.
- [ ] **Signup flow completes without errors.** Happy path only is fine, but it has to work every time. Test on mobile Safari — that's where most friends/family will open your link.
- [ ] **Verification flow has clear instructions.** Whether it's a real ID.me integration or a dummy verifier, the user needs to understand *why* they're verifying ("this proves you're a real person, not a bot") and *what to do* at each step.
- [ ] **Verification success is obvious.** A clear confirmation state — checkmark, badge, color change, something. "Did it work?" should never be a question.
- [ ] **Error states don't dead-end.** If something fails during signup or verification, show a message and a way forward. A blank screen or unhandled exception kills the demo instantly.

### Room Entry & Navigation

- [ ] **After verification, the path to a room is obvious.** Don't drop them on an empty dashboard. Either auto-navigate to the demo room or make the single room impossible to miss.
- [ ] **Pre-seeded demo room exists with an active poll.** Don't make people wait for an admin to create content. The room should already have a compelling topic ready to vote on.
- [ ] **The poll topic is something everyone has an opinion on.** Not a policy wonk topic. Something like "How should your city prioritize spending?" with dimensions like importance, urgency, feasibility. Your aunt needs to care.
- [ ] **Eligibility is clear.** If they can't enter a room, they need to know why ("You need to verify your identity first") with a link back to verification.

### Voting Experience

- [ ] **Sliders (or whatever input) are labeled with human-readable anchors.** "0.0 to 1.0" means nothing to a non-technical person. "Not at all important" to "Extremely important" works. Each end of each dimension needs a plain-English label.
- [ ] **Vote submission has clear feedback.** Button state change, a confirmation message, something. "Did my vote count?" is the first question everyone will ask.
- [ ] **One vote per user per dimension, updatable.** The upsert behavior needs to work. If someone moves a slider and resubmits, it should feel natural and not create a duplicate.

### Results

- [ ] **Results are visible after voting.** Even if it's just mean/median per dimension shown as a simple bar chart. The "aha moment" is seeing your input become part of a collective picture.
- [ ] **Results update when new votes come in.** Doesn't need to be real-time websocket push — a refresh or a poll interval is fine. But if two friends vote, they should be able to see the aggregate change.
- [ ] **Results are at least minimally interpretable without explanation.** Labels on axes, dimension names visible, vote count displayed. A bar chart with no labels is meaningless.

### Infrastructure

- [ ] **Demo environment is stable and publicly accessible.** You said you have this — just make sure it stays up through Mar 20 without manual intervention.
- [ ] **HTTPS works.** No certificate warnings. Non-technical people will not click through a browser security warning.
- [ ] **Page load is under 5 seconds.** Friends will open this on their phone over LTE. If it takes 10 seconds, they'll give up before the page renders.

---

## Should Have — Makes the Demo Land Better

These don't block the demo but meaningfully improve whether the concept clicks.

### For Everyone

- [ ] **A 2-3 sentence "how it works" blurb on the room page.** "This room is exploring [topic]. Move the sliders to share your perspective across multiple dimensions. Your vote is anonymous but verified." Saves you from writing individual explanations in every text message.
- [ ] **Visual distinction between "you haven't voted" and "you voted."** Grayed-out vs. colored, a checkmark, anything that shows status at a glance.
- [ ] **A result visualization that shows distribution, not just averages.** Even a simple dot plot or histogram per dimension. This is where people go "oh, interesting" vs. "ok, some numbers." The *shape* of opinion is the whole thesis.
- [ ] **Mobile-responsive layout.** Most friends/family will open this on their phone. The sliders especially need to work on a touch screen. Test on a real phone, not just browser dev tools.
- [ ] **A "what happens next" message after voting.** "Thanks for voting! Here's what the community thinks so far:" followed by results. Closes the loop.
- [ ] **Poll topic that seeds good conversation.** Pick 2-3 polls in the demo room on different topics. Gives people a reason to come back and compare perspectives. Local Kansas/Johnson County topics could work well for friends/family who share context.
- [ ] **Shareable link per room.** So someone can text it to a friend who also wants to try. Word-of-mouth is your distribution mechanism for the demo.

### For Technical Friends

- [ ] **A brief "how it's built" page or section.** These people will want to know the stack, the architecture, the trust model. A link to the whitepaper or a simplified version. Don't put this in the main flow — make it a footer link or an "About" page.
- [ ] **The endorsement/trust system is at least visible.** Even if it's not fully functional, showing that users have a trust level or verification badge signals the depth of the system to technical people who'll appreciate it.
- [ ] **Source code link (GitHub).** Technical friends will want to look at the code. If the repos are public, link them. If not, consider making them public for the demo period.

### For the Feedback Loop

- [ ] **A way to collect feedback in-app.** Even a simple "feedback" link that opens a Google Form or mailto link. Lowers the barrier from "I should tell Isaac about that thing" to one click.
- [ ] **Ask specific questions in your share message.** Don't just say "check this out." Say "I'm building this — can you try signing up, verifying, and voting? I specifically want to know: Was anything confusing? Did the results feel meaningful? Would you use this for real?" Directed questions get better feedback.

---

## Don't Waste Time On

These are tempting but won't help the demo land and will eat your remaining days.

### Infrastructure / DevOps

- [ ] ~~CI/CD pipeline improvements~~
- [ ] ~~Ephemeral staging environments (vCluster, etc.)~~
- [ ] ~~Load testing / k6 setup~~
- [ ] ~~Observability stack (Grafana, Loki, etc.)~~
- [ ] ~~Container registry optimization~~
- [ ] ~~AI exploratory testing harness~~
- [ ] ~~Automated smoke tests beyond what you already have~~

### Features

- [ ] ~~Pairwise comparison rooms~~
- [ ] ~~Batch-synthesized report rooms~~
- [ ] ~~AI persona participation~~
- [ ] ~~Tiered room escalation~~
- [ ] ~~Any Tier 2 or Tier 3 communication method~~
- [ ] ~~Real ID.me integration (if dummy verifier works for demo)~~
- [ ] ~~ZK ballots, federation, or anything from the future roadmap~~
- [ ] ~~Admin panel for creating rooms/polls (pre-seed them manually)~~

### Polish

- [ ] ~~Custom branding / logo design~~
- [ ] ~~Animation or transitions~~
- [ ] ~~Dark mode~~
- [ ] ~~Comprehensive error handling for every edge case~~
- [ ] ~~User settings / profile customization~~
- [ ] ~~Email notifications~~
- [ ] ~~Password reset flow (tell people to re-register if they forget)~~
- [ ] ~~Accessibility audit (important eventually, not for friends/family demo)~~
- [ ] ~~Performance optimization beyond "it loads"~~

---

## Parallel Workstreams

These workstreams are **independent of each other** and can be worked on by separate Claude sessions simultaneously. Each is tracked as a GitHub issue labeled `workstream/demo`.

**How to claim a workstream:** Run `gh issue list --label workstream/demo` and pick one not labeled `status/in-progress`. Then assign it and add the label:

```bash
gh issue edit <NUMBER> --add-label status/in-progress
```

When done, close the issue and open a PR.

### WS-V: Verification Flow *(in progress — separate thread)*

Owned by the demo-verification-flow plan. Covers:
- Verification flow with clear instructions
- Verification success is obvious (badge, confirmation)
- Eligibility gating on poll page ("You need to verify first")
- Navbar verification badge
- Settings verification section

**Files touched:** `service/src/sim/`, `service/src/bin/demo_verifier.rs`, `web/src/pages/Poll.page.tsx` (eligibility only), `web/src/components/Navbar/`, `web/src/pages/Settings.page.tsx`, `kube/`

### WS-A: Slider & Voting UX — [#450](https://github.com/icook/tiny-congress/issues/450)

Human-readable slider labels, vote submission feedback, voted/not-voted visual state.

**Current state:** Sliders show generic "Not at all" / "Extremely" marks. No confirmation after voting. No visual distinction between voted and not-voted.

**Checklist items:**
- [ ] Slider endpoints use dimension-specific labels from API data (min_label / max_label)
- [ ] Vote submission shows success toast or confirmation message
- [ ] Visual distinction between "haven't voted" and "voted" state
- [ ] "Update Vote" flow feels natural (not a duplicate)

**Files:** `web/src/pages/Poll.page.tsx` (VoteSlider component, vote submission handler)

### WS-B: Results Visualization — [#451](https://github.com/icook/tiny-congress/issues/451)

Make results interpretable and engaging — the "aha moment" of the demo.

**Current state:** Progress bars showing mean + stats table (mean/median/stddev). No auto-refresh. No distribution visualization.

**Checklist items:**
- [ ] Results are interpretable without explanation (labels, dimension names, vote count)
- [ ] Distribution visualization (dot plot, histogram, or violin per dimension) — not just averages
- [ ] Results auto-refresh on an interval (polling, not websockets)
- [ ] "What happens next" message after voting that leads into results

**Files:** `web/src/pages/Poll.page.tsx` (ResultsTable), possibly new chart component

### WS-C: Navigation & Post-Auth Flow — [#452](https://github.com/icook/tiny-congress/issues/452)

After login/verification, users should land somewhere useful — not a stub dashboard.

**Current state:** Dashboard is a stub. Navbar shows 4+ links to empty stubs (Analytics, Releases, Conversations). After login, no redirect to rooms.

**Checklist items:**
- [ ] After login, redirect to /rooms (or the demo room directly)
- [ ] Remove or hide navbar links to stub pages (Dashboard, Analytics, Releases, Conversations)
- [ ] Landing page → signup → rooms flow has no dead ends
- [ ] Error states on signup/login show a message and a way forward

**Files:** `web/src/Router.tsx`, `web/src/components/Navbar/Navbar.tsx`, `web/src/pages/Login.page.tsx`, `web/src/pages/Signup.page.tsx`

### WS-D: Demo Data & Poll Topics — [#453](https://github.com/icook/tiny-congress/issues/453)

Ensure seeded content is compelling and approachable for non-wonky friends/family.

**Current state:** Sim generates LLM-authored civic governance topics. May be too policy-focused. Topics need to be things "your aunt cares about."

**Checklist items:**
- [ ] At least one pre-seeded room with a universally relatable topic
- [ ] 2-3 polls per room with 3-5 dimensions each, human-readable labels
- [ ] Enough seeded votes that results look populated (not "1 vote")

**Files:** `service/src/sim/config.rs`, `service/src/sim/llm.rs`, `service/src/sim/votes.rs`

---

## Suggested Timeline

| Window | Focus |
|---|---|
| **Now → Weekend** | Get the core flow working end-to-end. Signup, verify, enter room, vote, see results. Sloppy is fine. |
| **Weekend → Mar 15** | Polish the must-haves. Fix the dead ends, add labels to sliders, make results visible. Test on your phone. |
| **Mar 15 → Mar 18** | Seed the demo room(s) with good topics. Write your share message with specific feedback questions. Do one dry run yourself pretending you've never seen it. |
| **Mar 18 → Mar 20** | Send it out. Resist the urge to fix one more thing. |

---

## The Real Checklist

- [ ] Did I send it to at least 5 people?
- [ ] Did I ask them specific questions?
- [ ] Did I resist the urge to explain it over their shoulder?
- [ ] Did I write down what surprised me about their feedback?
