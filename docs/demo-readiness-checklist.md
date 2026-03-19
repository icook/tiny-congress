# TinyCongress Demo Readiness Checklist

> **Target:** Friends & family demo by March 20, 2026
>
> **Core flow:** Signup → Verify identity → Enter room → Vote on multi-dimensional poll → See results

---

## Must Have — The Demo Breaks Without These

These are the things where, if missing, someone gets stuck or confused and closes the tab.

### Onboarding & Identity

- [x] **Landing page that explains what this is in one sentence.** Handled by separate landing page — not part of the app flow.
- [x] **Signup flow completes without errors.** E2E tests cover happy path including mobile Safari via smoke test. *(PR #503)*
- [x] **Verification flow has clear instructions.** Demo verifier at `demo-verify.tinycongress.com` with method selection UI (Government ID, Phone, Email). Users are redirected to a separate site that explains the step. *(PR #456)*
- [x] **Verification success is obvious.** Callback page shows green confirmation, navbar shows green "Verified" badge, settings page shows verification status and method. *(PR #456)*
- [x] **Error states don't dead-end.** Signup/login show error messages with a way forward. *(PR #503)*
- [x] **Login flow works end-to-end on mobile Safari.** Covered by smoke test `mobile-safari` Playwright project running against live demo URL. *(PR #527)*
- [x] **Signup → verify → vote completes in one session without re-login.** Covered by E2E smoke test flow. *(PR #527)*

### Room Entry & Navigation

- [x] **After verification, the path to a room is obvious.** Verification callback auto-redirects to `/rooms` after 2 seconds. Signup success screen shows "Verify Identity" and "Browse Rooms" buttons. *(PR #456)*
- [x] **Pre-seeded demo room exists with an active poll.** Sim worker seeds rooms and votes on schedule. *(WS-D — issue #453)*
- [x] **The poll topic is something everyone has an opinion on.** Compelling local topics seeded. *(WS-D — issue #453)*
- [x] **Eligibility is clear.** Poll page shows yellow alert "You need to verify your identity to vote" with a "Verify Now" button when authenticated but unverified. *(PR #456)*

### Voting Experience

- [x] **Sliders are labeled with human-readable anchors.** Dimension-specific min/max labels from API data. *(PR #458)*
- [x] **Vote submission has clear feedback.** Success toast on submission. *(PR #458)*
- [x] **One vote per user per dimension, updatable.** Upsert behavior works. Visual distinction between "Submit Vote" and "Update Vote" states. *(PR #458)*

### Results

- [x] **Results are visible after voting.** Distribution bar charts per dimension. *(PR #463)*
- [x] **Results update when new votes come in.** Auto-refresh on an interval. *(PR #463)*
- [x] **Results are at least minimally interpretable without explanation.** Labels, dimension names, vote count visible. *(PR #463)*

### Infrastructure

- [x] **Demo environment is stable and publicly accessible.** Smoke test runs on every master merge to verify. *(PR #527)*
- [x] **Demo verifier deployed and accessible.** Preflight health check validates `demo-verify.tinycongress.com` on every smoke run. *(PR #527)*
- [x] **HTTPS works.** Preflight curls all three domains over HTTPS; failures block the smoke run. *(PR #527)*
- [ ] **Page load is under 5 seconds.** Not explicitly measured — validate manually on a real device before Mar 20.

---

## Should Have — Makes the Demo Land Better

These don't block the demo but meaningfully improve whether the concept clicks.

### For Everyone

- [ ] **A 2-3 sentence "how it works" blurb on the room page.** "This room is exploring [topic]. Move the sliders to share your perspective across multiple dimensions. Your vote is anonymous but verified." Saves you from writing individual explanations in every text message.
- [x] **Visual distinction between "you haven't voted" and "you voted."** Voted state shown with different button text and color. *(PR #458)*
- [ ] **A result visualization that shows distribution, not just averages.** Even a simple dot plot or histogram per dimension. This is where people go "oh, interesting" vs. "ok, some numbers." The *shape* of opinion is the whole thesis.
- [ ] **Mobile-responsive layout.** Most friends/family will open this on their phone. The sliders especially need to work on a touch screen. Test on a real phone, not just browser dev tools.
- [ ] **A "what happens next" message after voting.** "Thanks for voting! Here's what the community thinks so far:" followed by results. Closes the loop.
- [ ] **Poll topic that seeds good conversation.** Pick 2-3 polls in the demo room on different topics. Gives people a reason to come back and compare perspectives. Local Kansas/Johnson County topics could work well for friends/family who share context.
- [ ] **Shareable link per room.** So someone can text it to a friend who also wants to try. Word-of-mouth is your distribution mechanism for the demo. Verify room URLs work when pasted into a text message and resolve without auth walls before content is visible.
- [ ] **"Why verify?" copy on signup success screen.** The verify button exists, but non-technical users need one sentence explaining why ("so we know you're a real person, not a bot").
- [ ] **Graceful fallback if demo verifier is down.** If the verifier pod crashes, "Verify Now" buttons across the app lead to a broken page. Consider health-check or degradation message.

### For Technical Friends

- [ ] **A brief "how it's built" page or section.** These people will want to know the stack, the architecture, the trust model. A link to the whitepaper or a simplified version. Don't put this in the main flow — make it a footer link or an "About" page.
- [x] **The endorsement/trust system is at least visible.** Navbar shows verification badge. Settings page shows verification status, method, and date. *(PR #456)*
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

### WS-V: Verification Flow *(complete — PR #456)*

~~Owned by the demo-verification-flow plan. Covers:~~
- ~~Verification flow with clear instructions~~
- ~~Verification success is obvious (badge, confirmation)~~
- ~~Eligibility gating on poll page ("You need to verify first")~~
- ~~Navbar verification badge~~
- ~~Settings verification section~~

### WS-A: Slider & Voting UX *(complete — PR #458)* — [#450](https://github.com/icook/tiny-congress/issues/450)

~~Human-readable slider labels, vote submission feedback, voted/not-voted visual state.~~

### WS-B: Results Visualization *(complete — PR #463)* — [#451](https://github.com/icook/tiny-congress/issues/451)

~~Distribution bar charts per dimension with auto-refresh. Results interpretable without explanation.~~

### WS-C: Navigation & Post-Auth Flow *(complete — PRs #503, #508, #511)* — [#452](https://github.com/icook/tiny-congress/issues/452)

~~After login redirect to /rooms, stub nav links removed, error states improved.~~

### WS-D: Demo Data & Poll Topics — [#453](https://github.com/icook/tiny-congress/issues/453) *(closed)*

~~Ensure seeded content is compelling and approachable for non-wonky friends/family.~~

### WS-E: Mobile & Cross-Browser Validation *(complete — PR #527)* — [#460](https://github.com/icook/tiny-congress/issues/460)

~~Covered by smoke test running `mobile-safari` and `mobile-chrome` Playwright projects against the live demo URL on every master merge.~~

### WS-F: Demo Environment Smoke Test *(complete — PR #527)* — [#461](https://github.com/icook/tiny-congress/issues/461)

~~Preflight job validates HTTPS on all three domains and sim worker content. Smoke matrix runs full E2E suite. Triggers automatically after CI passes on master.~~

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
