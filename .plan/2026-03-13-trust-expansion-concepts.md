# Trust System Expansion Concepts

**Date:** 2026-03-13
**Purpose:** Brainstorming ideas for trust system capabilities beyond the core mechanism design, organized by the scale at which each becomes most valuable. These are not requirements — they're options to explore as the system grows.

**Relationship to other docs:**
- Scale readiness matrix: defines gates that MUST pass. This doc defines capabilities that COULD be built.
- Open questions: design spikes for known gaps. This doc captures broader ideas that haven't been scoped yet.
- Red team threat model: attacks to defend against. Several concepts here are responses to specific threat vectors.

---

## Architecture-level (explore now, before scale locks in assumptions)

### Anchor-free trust scoring — SPIKE COMPLETED, RESULTS NEGATIVE

**Idea:** Replace anchor-relative scoring (Dijkstra distance + max-flow diversity from a single root) with a global reputation system that has no distinguished root node.

**Spike results (2026-03-13):** Implemented EigenTrust and PageRank, tested against adversarial topologies. **Both fail catastrophically at Sybil detection.** See `.plan/2026-03-13-anchor-problem-statement.md` for full analysis.

| Scenario | Anchor-relative | EigenTrust/PageRank |
|---|---|---|
| Sybil mesh (1 bridge) | 1.36x blue advantage | 0.95x — **Sybils score higher** |
| Sybil mesh (2 bridges) | 1.36x blue advantage | 0.94x — **Sybils score higher** |
| Colluding ring | 5.4x blue advantage | 0.68x — **ring inflates scores 1.46x** |

**Root cause:** Anchor-free algorithms can't distinguish "endorsed by many real people" from "endorsed by many fake people endorsing each other." Dense internal mesh endorsements create self-reinforcing trust amplification loops. The anchor provides a reference point that breaks this symmetry — trust must originate from a known-good source.

**EigenTrust alpha sensitivity:** Low alpha (graph-driven) → Sybils score higher. High alpha (pretrust-driven) → scores converge to uniform (algorithm adds no value). No sweet spot exists.

**Launch decision (2026-03-15):** Founder is the trust root at launch — pragmatic and accepted. Multi-anchor migration is Tier 3+ work (~10k users). Options for that migration:
1. **Multi-anchor consensus** — compute from N anchors, require agreement. Preserves security reduction, distributes failure.
2. **Personalized PageRank** — anchor-relative PageRank (used by SybilRank). Still requires an anchor but may offer smoother scoring.
3. **Rotating/electable anchor** — keep the concept but make it a governance role, not a permanent identity.
4. **Community detection hybrid** — use graph clustering to detect Sybil boundaries as a supplementary signal.

Research sweep completed — see `.plan/2026-03-13-anchor-problem-statement.md` for full analysis.

**Addresses:** Red team A5 (anchor compromise), open question Q28 (anchor redundancy), Q29 (anchor-free scoring)

### PageRank / EigenTrust as supplementary signals — REFRAMED BY SPIKE

**Idea:** Run PageRank and EigenTrust as additional signals alongside distance/diversity. Originally proposed for Sybil detection and community influence scoring.

**Spike finding:** EigenTrust/PageRank **cannot** separate Sybils from legitimate nodes (Sybils score equal or higher). However, they retain value for a different purpose: **anomaly detection.** A sudden change in a node's PageRank indicates a topology shift — useful for detecting compromised accounts or Sybil cluster formation — even if the absolute score doesn't reliably separate real from fake.

**Revised value proposition:**
- ~~Sybil detection (low PageRank + high diversity = suspicious)~~ — INVALIDATED. High PageRank correlates with Sybil mesh density, not legitimacy.
- **Anomaly detection** (sudden PageRank change = topology shift) — VALID. Relative changes over time are meaningful even if absolute scores aren't.
- ~~Community influence score for UI~~ — QUESTIONABLE. If Sybils can inflate their scores, displaying these to users is misleading.

**Feasibility:** Still trivial at target scales (<1 second for 100k nodes). Spike measured 258ms for EigenTrust, 316ms for PageRank at 1k nodes.

**Recommended use:** Nightly batch anomaly detection, not eligibility scoring.

---

## Valuable at 1k–5k users

### Flags (lighter than denouncement)

**Idea:** Public signal that someone is suspect. No graph mutation — purely informational. "I have concerns but I'm not revoking my endorsement."

**Why:** Denouncement budget (d=2) is scarce and irreversible (revokes your edge). Flags express graduated distrust without burning a finite resource. A flag says "thin ice" where a denouncement says "bridge burned."

**Design space:**
- Cost: free? Limited budget? Costs reputation?
- Visibility: public to all? Visible only to direct endorsers? Aggregated anonymously?
- Effect: purely informational, or triggers some system response at threshold?
- Reversible: yes (unlike denouncement edge revocation)

**Addresses:** The gap between "everything is fine" and "I'm spending one of my 2 denouncements." Gives the social graph a low-cost signaling mechanism.

### Key ownership liveness tests

**Idea:** Periodically challenge accounts to prove they still control their keys. Accounts that can't prove liveness get restricted from sensitive actions (endorsing, voting).

**Why:** Directly addresses account compromise (red team A2). An inactive account is vulnerable — if nobody's home, an attacker who gains the keys faces no resistance. Liveness tests don't catch attackers who have the keys, but they identify *which accounts are vulnerable* (dormant, unresponsive).

**Design space:**
- Challenge frequency: monthly? Quarterly? Triggered by suspicious signals?
- Restriction scope: can't endorse? Can't vote? Reduced weight on outbound edges?
- Interaction with decay: liveness failure could accelerate decay (your edges are less trustworthy if you're not paying attention)
- UX: passive (device key signs automatically when app is open) vs active (user must explicitly confirm)

**Addresses:** Red team A2 (account takeover), account compromise blind spot

### Group-based endorsement

**Idea:** Rooms/groups can collectively endorse: "our community vouches for this person." Different trust semantics — a group endorsement means "this person is part of our community" rather than "I personally trust this person."

**Why:** Individual endorsements are bottlenecked by slot budget (k=10). A community that wants to welcome a new member shouldn't require 2+ individuals to each spend a slot. Group endorsement provides a different trust channel with different properties.

**Design space:**
- Weight: lower than individual QR endorsement? Configurable per room?
- Quorum: how many room members must agree? Simple majority? Supermajority?
- Diversity contribution: does a group endorsement count as one path or N paths?
- Slot cost: does it consume a slot from anyone? From the room?
- Revocation: can the room revoke? Does individual departure from the room affect it?

**Mentioned in TRD** as a planned add-on. Likely one of the first major feature expansions.

### Additional endorsement tiers (wider, less impactful)

**Idea:** A "light endorsement" tier — endorse strangers with low weight and low risk. Distinct from full endorsement: "I met this person once" vs "I vouch for this person."

**Why:** Addresses the diversity problem for tight communities. If SBM testing (#680) shows clustered communities have diversity=1 because all paths go through one bridge, light cross-community endorsements create additional independent paths cheaply. The weight table (ADR-023) already supports low weights (email=0.1) — this might be primarily a UX change: make it easy and encouraged to give lightweight endorsements to acquaintances.

**Design space:**
- Separate slot budget? (e.g., k=10 full endorsements + k=20 light endorsements)
- Shared budget but lower cost? (light endorsement costs 0.5 slots?)
- Same mechanism, different UX encouragement?
- Different decay rate? (light endorsements decay faster — acquaintances fade)

**Addresses:** SBM diversity problem, inter-community connectivity, verifier slot budget problem (Q30 — light endorsement tier is the leading option for how verifier entities participate in the graph without requiring k=∞)

---

## Valuable at 5k–10k users

### Deluminants (silences / injunctions)

**Idea:** Temporary privilege freeze. "This account can't endorse, vote, or use trust-gated features until the pause is lifted." Reversible. Doesn't mutate the graph.

**Why:** Fills the gap between "flag someone" (no enforcement) and "denounce someone" (irreversible graph change). When something looks wrong but isn't proven, you need a precautionary hold while investigation happens.

**Design space:**
- Who can impose: automated (triggered by flag threshold)? Governance (room vote)? Deputized moderators?
- Duration: fixed time? Until lifted by imposer? Until cleared by investigation?
- Scope: all privileges? Just endorsement? Just voting?
- Appeal: immediate? Requires room consensus to lift?

**Addresses:** The "we need to act now but don't know the full picture" gap. Precursor to adjudication (Q26).

### Pre-computing future potentials

**Idea:** "What-if" analysis on the trust graph. What happens if node X is compromised? What if these 3 accounts all endorse this new account? Run hypothetical max-flow computations to anticipate topology changes before they happen.

**Why:** Detection/early-warning tool. At 10k+, you want to know "if account X is compromised, 47 nodes lose eligibility" BEFORE it happens. Enables proactive defense rather than reactive incident response.

**Design space:**
- Scope: pre-compute for all nodes (expensive at scale)? Only for high-centrality nodes? On-demand?
- Triggers: nightly batch? On each endorsement? On flag/suspicion?
- Output: alert to admins? Automated response? Dashboard metric?
- Scenarios: compromise, departure, coordinated denouncement, mass decay

**Feasibility:** Computationally trivial at 100k (run the existing engine with modified inputs). Could run nightly as a batch job. At 1M+ would need selective computation (high-centrality nodes only).

### Adversarial investigation / social auditing

**Idea:** When someone is flagged, the system prompts their endorsers to investigate. "3 people have flagged Bob. Alice, you endorsed Bob — can you check in?" Turns the trust graph into a detection network.

**Why:** The people closest to a suspect are best positioned to investigate. No automated heuristic can match "I called Bob and something seemed off" or "I saw Bob at the event last week, he's fine." Leverages existing social trust for detection.

**Design space:**
- Prompt mechanism: notification? In-app message? Required before endorsement renewal?
- Outcome: endorser can affirm ("Bob's fine"), flag ("something's wrong"), or revoke endorsement
- Privacy: does the flagged person know they're under investigation?
- Threshold: how many flags trigger investigation prompts?
- Relationship to deluminants: does investigation auto-freeze the account?

**Addresses:** Account compromise detection (A2), social engineering detection (A3), the "no post-creation defense" blind spot

---

## Valuable at 10k+ users

### First-class rooms for trust consensus

**Idea:** Model certain self-governing aspects as rooms that facilitate trust-system-critical consensus. A room can collectively decide trust actions (endorse, flag, freeze, adjudicate) through its normal governance mechanisms.

**Why:** Solves the adjudication design problem (Q26) by reusing existing room mechanics rather than designing a bespoke governance process. Instead of "what quorum of trusted users is required to slash someone?", the answer is "the room votes on it."

**Design space:**
- Which rooms have trust powers? All? Only designated "trust council" rooms? Rooms that meet trust-score thresholds?
- Scope of power: endorsement only? Flags? Deluminants? Full slashing?
- Relationship to trust scores: room members' trust scores determine the room's authority?
- Appeals: cross-room appeal (a higher-trust room can override)?

**Addresses:** Adjudication (Q26), governance capture (D1/D2), "who decides?" for severe trust actions

### Human heuristic police

**Idea:** Deputize trusted users to crowdsource moderation and resilience. Users with high trust scores can flag, investigate, and escalate. The trust graph determines who has moderation authority — the same system that measures trust also determines who can protect it.

**Why:** Automated heuristics (Q24) have known evasion strategies. Human judgment catches things algorithms miss. But unstructured moderation doesn't scale. This provides structure: the trust graph itself determines who has authority to act.

**Design space:**
- Authority tiers: flag (anyone) → investigate (high trust) → freeze (very high trust) → slash (room consensus)
- Incentives: does moderation activity count toward trust renewal? Does it consume time/attention?
- Abuse: what prevents a high-trust moderator from weaponizing their authority?
- Relationship to rooms: moderators operate within rooms? Across rooms?
- Accountability: moderation actions are logged and reviewable?

**Addresses:** Sybil detection at scale, the "operational security is never done" problem, governance capture resistance

---

## Cross-cutting notes

**Several ideas compose naturally:**
- Flags + adversarial investigation + deluminants = a graduated response system (signal → investigate → freeze → adjudicate)
- First-class rooms + human heuristic police = community-driven trust governance
- EigenTrust/PageRank + pre-computing potentials = rich graph analytics for detection and early warning
- Liveness tests + decay = stronger account hygiene (inactive accounts lose weight AND get restricted)
- Light endorsement tiers + group endorsement = more paths, more diversity, lower barrier to connection

**The anchor question is foundational — and now empirically constrained.** The EigenTrust/PageRank spike (2026-03-13) demonstrated that naive anchor-free scoring fails at Sybil detection. The anchor provides a critical security property (breaking self-reinforcement symmetry) that global algorithms cannot replicate. The path forward is not "remove the anchor" but "make the anchor more accountable and redundant" — multi-anchor consensus, rotating anchors, or community-elected anchor sets. See `.plan/2026-03-13-anchor-problem-statement.md`.

**EigenTrust/PageRank are NOT useful as eligibility signals** but retain value for **anomaly detection** (topology shift monitoring). The "rich graph analytics" composition above should be reframed: PageRank changes over time (not absolute values) are the useful signal.

**Computational budget at target scales:** At 100k users with k=10, the entire graph is ~16 MB. You could run PageRank, EigenTrust, community detection, centrality computation, what-if analysis for every node, and anomaly detection — all in under a minute on a single core. Computation is not the constraint; design and governance are.
