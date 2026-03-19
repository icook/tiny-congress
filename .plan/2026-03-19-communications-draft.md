# 30-Day Communications Draft

**Date:** 2026-03-19
**Status:** Draft -- edit and personalize before sending
**Context:** Three versions of a "here's what I built in 30 days" update, tailored to different audiences.

---

## Version 1: Friends & Family

**Subject: I built a thing -- come try it**

Hey,

For the last month I've been working full-time on a project called TinyCongress. I wanted to tell you about it and ask you to try it out.

The short version: I'm building a place where communities can actually figure out what they think about things. Not a social media platform, not a forum -- a structured way for groups of real people to vote, discuss, and see where they actually agree and disagree.

The reason I think this matters: right now, if you want to know what people in your neighborhood, your company, or your city actually think about something, there's no good way to find out. Polls are gameable. Social media rewards the loudest voices. Town halls are attended by 12 people. Genuine public opinion is basically invisible.

TinyCongress tries to fix that by making identity real. When you sign up, your browser generates a cryptographic key pair -- think of it like a digital signature that proves you're you. No one can vote as you, and no one can create fake accounts at scale, because every account is anchored to a real trust relationship. You endorse people you actually know, and that web of trust is what makes the whole system work.

What's working right now: you can sign up, get endorsed by someone you know (there's a QR code handshake -- it's kind of fun), enter a room, vote on questions, and see real-time results. The first room is about brand ethics -- you're comparing companies and saying which ones you think are more ethical. It's a small starting point, but the mechanics generalize to any kind of structured community decision.

I'd love it if you'd try it out. It's live at tinycongress.com. Sign up, poke around, and tell me what's confusing. Seriously -- if something doesn't make sense, that's the most useful feedback I can get right now.

Thanks for humoring me.

---

## Version 2: Technical Peers

**Subject: 30 days of solo civic tech -- architecture notes**

I took 30 days off to build TinyCongress, a community governance platform anchored to cryptographic identity. It's live at tinycongress.com. Here's what I think is technically interesting about it.

**The trust model**

The core question is Sybil resistance without a central authority. I started where most people start -- looking at EigenTrust and PageRank-style approaches. I built a simulation harness and ran them against adversarial scenarios. They failed. Specifically: a coordinated group of 20 Sybil accounts could game a PageRank-derived trust score within 3 cycles by creating a densely-connected cluster and establishing a few bridge edges to legitimate nodes. The fundamental problem is that these algorithms treat trust as a flow property of the graph, which means anyone who can manufacture edges can manufacture trust.

What I landed on instead: discrete endorsement slots. Every account gets k=10 endorsement slots. Each slot carries variable weight based on relationship depth, but you can't create more of them. This means a Sybil operator controlling N fake accounts can endorse at most 10N targets -- and each of those endorsements traces back to a real account that's putting its own reputation on the line. Denouncement has fail-closed semantics: if you denounce someone, the edge is immediately revoked, and only the denouncer can restore it.

I validated this against 12 attack vectors across 4 architecture decision records, using a simulation harness that models trust propagation, coalition attacks, and edge manipulation. The system holds at 5k users with high confidence, 10k medium, and the scaling properties map cleanly to 100k.

**The crypto boundary**

tc-crypto is a single Rust crate that compiles to both native (backend validation) and WASM (browser key generation, signing, envelope encryption). The server never sees private key material -- it's a dumb witness. Device keys are non-extractable CryptoKeys. Backup envelopes use Argon2id with OWASP 2024 minimums, encrypted client-side, stored as opaque blobs. The WASM module loads lazily behind a CryptoProvider so components that need crypto declare the dependency explicitly.

This isn't just a design preference -- the trust model falls apart if the server can sign on behalf of users. Making it structurally impossible (not just policy-impossible) is the point.

**Infrastructure**

I self-host everything on bare-metal Kubernetes (Talos OS + Flux CD). This includes the CI system -- a 4-tier ARC fleet with per-pod Docker-in-Docker, BuildKit with NVMe cache, a Zot OCI registry, and Garage S3 for GitHub Actions cache. The whole stack is GitOps-managed with full observability (kube-prometheus-stack, Grafana Alloy, ServiceMonitors on everything).

Why self-host CI? Cost, but also control. When your CI runner has a local OCI cache and a BuildKit daemon with warm layers, Docker builds that take 8 minutes on GitHub-hosted runners take 90 seconds. For a solo dev iterating fast, that's the difference between flow state and context-switching.

**AI-assisted development**

I used Claude extensively for an automated refinement pipeline: batched 30+ PRs to add handler-level tests, introduce newtypes, and consolidate service layers. The key insight was treating AI as a batch worker, not a pair programmer -- define the pattern once, then apply it mechanically across the codebase. This graduated the entire identity module (32 PRs) from stringly-typed handlers to newtype-enforced domain boundaries.

The repo is at github.com/icook/tiny-congress if you want to look at the code. Happy to go deeper on any of this.

---

## Version 3: Civic Tech / Potential Early Users

**Subject: What if we could actually see what people think?**

Here's something that bothers me: genuine public opinion is invisible.

We have more communication tools than at any point in human history, and we're worse at understanding what communities actually believe. Social media optimizes for engagement, not signal. Polls are snapshots that die on contact with motivated respondents. Town halls select for the people with time and confidence to show up. Comment periods are captured by organized interests.

The result is that most civic decisions are made in an information vacuum. Leaders don't know what their constituents think. Communities don't know what they agree on. And the gap gets filled by whoever is loudest.

I spent the last 30 days building TinyCongress to try to change that.

**The approach**

TinyCongress is a platform for structured community decision-making. "Structured" means: specific questions, clear choices, transparent aggregation. Not open-ended discussion (we have plenty of that), but the kind of focused input that's actually useful for decisions.

The key design choice is identity-anchored communication. When you join TinyCongress, you create a cryptographic identity -- a digital signature that's uniquely yours. You build trust by endorsing people you know in real life (there's a QR code handshake). This web of real relationships is what prevents the platform from being flooded with fake accounts or coordinated manipulation.

This matters because every other approach to online civic input has the same failure mode: the people who show up aren't representative, and there's no way to tell real participants from manufactured ones. Verified identity (without requiring government ID or personal data disclosure) is the prerequisite for trustworthy aggregation.

**What's working today**

The platform is live at tinycongress.com. You can:

- Sign up and create your cryptographic identity (takes about 30 seconds, no personal information required)
- Build trust by endorsing people you know
- Enter rooms -- structured spaces organized around specific topics
- Vote on questions and see real-time, transparent results
- See your trust score and understand exactly why you have the access you have

The first room is about brand ethics -- comparing companies and voting on which are more ethical. It's a deliberately low-stakes starting point, but the same mechanics work for neighborhood budget priorities, organizational strategy decisions, or policy preferences.

**What's different**

Most civic tech projects start with the interaction design and bolt on identity later (or never). TinyCongress starts with identity and trust, because without those, every interaction is suspect. The trust system was validated against 12 different attack scenarios, from Sybil floods to coordinated manipulation to trust chain exploitation. It's designed to fail closed -- when in doubt, restrict access rather than grant it.

Every vote, every endorsement, every interaction is cryptographically signed and computationally reducible to a transparent aggregate. No black boxes. No algorithmic curation. The output is exactly what the participants put in.

**What's next**

Right now this is a working demo, not a finished product. I'm looking for early users who care about civic infrastructure and want to help shape what this becomes. If you try it out and something is confusing, broken, or missing -- that's exactly the feedback that matters most at this stage.

tinycongress.com

---

*These are drafts. Edit for voice, add personal details, adjust length for medium (email vs. message vs. post).*
