# Red Team Threat Model: Breaking Trust from the Attacker's Perspective

**Date:** 2026-03-13
**Premise:** The system is broken. You just need to show how.
**Approach:** Top-down enumeration of every path to getting a fake identity trusted, weaponizing denouncement, or degrading the trust graph. Assumes a motivated attacker with moderate resources.

---

## Attacker goals

An attacker wants one or more of:
1. **Forge trust:** Get a Sybil identity to pass eligibility (distance ≤ 5.0, diversity ≥ 2)
2. **Destroy trust:** Remove a legitimate user's eligibility
3. **Degrade the graph:** Reduce overall graph health (reachability, connectivity) to undermine the system's usefulness
4. **Capture governance:** Control enough trusted identities to influence votes or adjudication

---

## Attack surface 1: Forging trust (getting a Sybil past eligibility)

### A1. Purchase endorsements from legitimate users

**How:** Find 2+ users on independent paths from anchor willing to endorse your Sybil account. Pay them, deceive them, or exploit social pressure.

**Why it works:** The QR handshake only ensures physical copresence — it doesn't verify the endorser is making a genuine trust judgment. A user who endorses anyone for $20 is indistinguishable from one who endorses carefully.

**Cost:** Decreases with population. If willingness-to-sell is normally distributed, the cheapest 2 accounts on independent paths get cheaper as N grows. At 100k users, the tail could be very cheap.

**Current defenses:** Diversity threshold (need 2+ independent paths). Slot limit (k=10 caps endorsement fanout). Decay (purchased endorsements eventually expire).

**Gaps:** No detection mechanism for purchased endorsements. No way to distinguish "I trust this person" from "I was paid to endorse this person." The system is structurally blind to endorsement quality beyond the endorser's own trust position.

**Simulation testable?** Partially. Can model cost distributions and test how many Sybils pass at various price points. Cannot model social dynamics of endorsement-selling.

### A2. Compromise existing endorsed accounts (account takeover)

**How:** Credential stuffing, phishing, social engineering, device theft, or purchasing account credentials. Attacker gains control of an already-endorsed account and inherits its full graph position.

**Why it works:** The endorsement ceremony validates the *account creation*, not ongoing control. Once an account is endorsed, changing who controls it doesn't change the trust graph. The compromised account has real endorsements from real people who vouched for the original owner.

**Cost:** Potentially very low at scale. Automated credential attacks are a commodity. Device theft gives access to device keys. If the backup envelope is password-protected with a weak password, it can be brute-forced (Argon2id makes this expensive but not impossible).

**Current defenses:** Ed25519 keys never leave the browser (harder to phish than passwords). Backup envelope uses Argon2id with OWASP 2024 minimums. Device key revocation exists (but requires root key access, which the attacker may also have).

**Gaps:**
- **No anomaly detection for behavior change.** If an account suddenly changes its endorsement pattern, voting behavior, or activity level, nothing flags it.
- **No "endorsement review" after compromise.** If Alice endorsed Bob, and Bob's account is compromised, Alice's endorsement of "Bob" now applies to the attacker. Alice has no way to know this happened.
- **Root key compromise is total.** If the attacker gets the root key (via backup envelope brute-force or device theft), they control the account permanently — they can revoke the original owner's device keys and issue their own.
- **No account recovery.** If someone loses their root key to an attacker, there's no recovery path. The identity is gone.

**Simulation testable?** Yes — can model blast radius of compromising high-centrality accounts. Cannot model the difficulty of actual credential attacks.

### A3. Social engineering the endorsement ceremony

**How:** Attend the same event as the target. Present yourself as a legitimate community member. Complete the QR handshake. The target has no way to verify your "real" identity — the ceremony only proves physical copresence, not identity truthfulness.

**Why it works:** The endorsement ceremony is designed to be low-friction. "Scan my QR code" doesn't involve background checks. At a community event with 50 people, endorsing someone you just met feels natural.

**Cost:** Low — requires attending events and being personable. This is the classic in-person social engineering attack.

**Current defenses:** Weight table (ADR-023) — a text-message endorsement from an acquaintance has weight 0.1-0.2, much less than a QR endorsement from a years-long relationship. This reduces the graph impact of shallow endorsements.

**Gaps:**
- **The weight is self-reported.** The endorser selects relationship depth. A manipulated endorser can be convinced to select "deep trust" for a new acquaintance.
- **2 events = 2 independent paths.** If the attacker attends 2 different community events and gets endorsed by 1 person at each (who are on independent paths), the Sybil passes diversity ≥ 2.
- **The QR handshake doesn't record the social context.** There's no metadata about "where did this endorsement happen" that could be used for later audit.

**Simulation testable?** Partially. Can model the graph impact of shallow endorsements at various weights. Cannot model the social dynamics of event-based social engineering.

### A4. Sybil mesh with purchased bridges

**How:** Combine A1 (purchase endorsements) with creating a mesh of fake accounts. The purchased endorsements are the "bridges" into the legitimate graph. The mesh provides additional fake-to-fake endorsements for graph density.

**Why it works at scale:** At 1k users, this doesn't help — diversity = bridge_count regardless of mesh size. But the mesh provides operational flexibility: the attacker has multiple identities inside the system and can use them for voting, governance capture, or further social engineering.

**Cost:** bridge_count × price_per_endorsement + N × account_creation_cost. The mesh itself is free (attacker controls all accounts). The bridges are the expensive part.

**Current defenses:** diversity = bridge_count (proven). k=10 slot cap limits each bridge's endorsement fanout into the mesh.

**Gaps:** Once bridge_count ≥ 2, the mesh provides diversity ≥ 2 for ALL mesh members. A single investment of 2 purchased endorsements can grant eligibility to an arbitrarily large number of Sybil identities. The slot cap limits how many mesh nodes each bridge can directly endorse (10), but mesh-internal endorsements can create paths from bridge → mesh_1 → mesh_2 → ... extending reach.

**Simulation testable?** Yes — this is exactly what the existing Sybil mesh tests model. The gap is testing with mesh_size >> 10 where indirect paths through the mesh extend beyond direct bridge connections.

### A5. Exploit the anchor bootstrap

**How:** If the anchor account is compromised, the entire trust graph is rooted in an attacker-controlled identity. All trust measurements are relative to anchor.

**Why it works:** The anchor is a single point of failure by design. All distance and diversity computations start from the anchor.

**Cost:** Extremely high if the anchor is well-protected. But "well-protected" is an operational assumption, not a structural guarantee.

**Current defenses:** The anchor account presumably has strong key management. Multi-anchor or distributed anchor is a potential future mitigation.

**Gaps:** Single anchor = single point of failure. No multi-anchor design exists. No anchor rotation procedure. No detection for anchor compromise.

**Simulation testable?** No — this is an operational security question, not a graph theory question.

---

## Attack surface 2: Destroying trust (removing a legitimate user's eligibility)

### B1. Coordinated denouncement

**How:** Multiple Sybil accounts denounce the target. With denouncer-only revocation, each denouncement only revokes the denouncer's own edge — but Sybils might not have edges to the target. If they do (because the target endorsed them back), each denouncement removes one path.

**Why it might work:** If the target has low diversity (diversity=2) and the attacker controls 2 accounts that the target has endorsed, 2 denouncements would drop diversity to 0.

**Current defenses:** Denouncer-only revocation means the attacker must have an edge TO the target. The denouncer's edge is what gets revoked — you can only denounce someone you've endorsed. d=2 denouncement budget limits each attacker account to 2 denouncements.

**Gaps:** The defense assumes Sybils don't have edges to the target. But in A3 (social engineering), the attacker could first get endorsed BY the target at an event, then later denounce them. This is a two-phase attack: infiltrate, then weaponize.

**Simulation testable?** Yes — build scenario where attacker first gets endorsed by target via A3, then denounces.

### B2. Starve endorsement slots

**How:** Get the target to endorse 10 Sybil accounts (filling all k=10 slots). The target now has no remaining slots for legitimate endorsements. Then those Sybil accounts go inactive, and the edges decay — but the target's slots are occupied by decaying-but-not-yet-released edges.

**Why it works:** If auto-release threshold is too high or doesn't exist, dead edges permanently consume slots. The target's graph connectivity degrades as legitimate edges can't be formed.

**Current defenses:** ADR-025 auto-release below weight 0.05. Step decay means edges hit 0.0 after 2 years, auto-releasing the slot.

**Gaps:** 2-year window is long. During that time, the target has reduced slot availability. If the target filled 5 slots with Sybils at an event, they lose half their endorsement capacity for up to 2 years.

**Simulation testable?** Yes — model slot exhaustion attack and measure impact on target's diversity over time.

### B3. Bridge node targeting

**How:** Identify and compromise the bridge nodes between communities. If community A connects to the anchor through bridge nodes X and Y, compromising X and Y disconnects the entire community.

**Why it works:** In community-structure topologies, inter-community bridges are rare and critical. Targeted removal has disproportionate impact.

**Current defenses:** BA graph testing shows resilience (99.7% after removing top 3). But BA has many more bridges than real community-structure graphs.

**Gaps:** Not tested on SBM topology where bridges are naturally scarce. This is a primary risk for #680.

**Simulation testable?** Yes — SBM bridge identification and targeted removal.

---

## Attack surface 3: Degrading the graph

### C1. Endorsement spam (slot exhaustion at scale)

**How:** Create many accounts, attend many events, endorse everyone. Fill legitimate users' endorsement slots with low-value edges. This degrades the graph's signal-to-noise ratio.

**Current defenses:** k=10 slot cap means each user chooses their endorsements. Weight table means shallow endorsements (acquaintance, text message) contribute less. Decay clears stale edges.

**Gaps:** The user makes the endorsement decision. At a community event, social pressure to "just scan and endorse" could lead to slot waste. No UX guidance about slot scarcity or endorsement quality.

### C2. Decay-based erosion

**How:** Do nothing. Let the graph decay naturally. In communities that don't actively re-endorse, edges lose weight and eventually auto-release. Over 2+ years, inactive communities disconnect from the anchor.

**Why it works:** This isn't an "attack" — it's the intended behavior. But it could be exploited: an attacker who can prevent re-endorsement (by disrupting community events, for example) can let decay do the disconnection work.

**Current defenses:** Re-swap flow exists. Decay is gradual (1yr full, 2yr half, 2yr+ zero).

**Gaps:** No proactive notification ("your endorsement of Bob expires in 3 months — consider re-endorsing"). Users must remember to renew. Communities that meet infrequently (annually?) may lose all internal edges.

---

## Attack surface 4: Governance capture

### D1. Sybil voting bloc

**How:** Get N Sybil identities past eligibility (via A1-A4). Use them as a coordinated voting bloc to influence governance decisions. At sufficient scale, Sybils can outvote legitimate users.

**Cost:** N × cost_per_sybil_identity. If each identity needs 2 purchased endorsements at $X each, total cost = N × 2 × X.

**Current defenses:** Diversity threshold limits Sybil creation rate. Adjudication process (future) could revoke proven Sybils.

**Gaps:** No vote-pattern analysis. No detection of coordinated voting. No Sybil-resistant voting mechanism (e.g., quadratic voting, conviction voting). The trust system determines WHO can vote but not HOW votes are counted — a Sybil bloc with 100 eligible identities gets 100 votes.

### D2. Adjudication capture

**How:** Get enough Sybil identities with high diversity/low distance to control the adjudication quorum. Use them to protect other Sybils from slashing or to slash legitimate users.

**Cost:** Requires Sybils with genuinely high trust scores — expensive per identity but devastating if achieved.

**Current defenses:** Adjudication design hasn't started. No quorum mechanism exists yet.

**Gaps:** Everything. This attack surface is entirely unaddressed because the governance layer doesn't exist.

---

## Threat priority matrix

| Attack | Feasibility at 1k | Feasibility at 100k | Detection difficulty | Impact | Priority |
|---|---|---|---|---|---|
| **A1. Purchase endorsements** | Hard ($$$) | Easy ($) | Very hard | High (Sybil entry) | **P0 at scale** |
| **A2. Account takeover** | Moderate | Easy (automated) | Hard (no anomaly detection) | Critical (inherits full trust) | **P0** |
| **A3. Social engineering ceremony** | Easy (attend event) | Easy (attend events) | Very hard (legitimate-looking) | Moderate (low weight) | **P1** |
| **A4. Sybil mesh + bridges** | Hard (need bridges) | Moderate (cheaper bridges) | Moderate (structural heuristic) | High (amplifies A1) | **P1** |
| **A5. Anchor compromise** | Very hard | Very hard | Easy (total failure) | Catastrophic | **P2** (operational) |
| **B1. Coordinated denouncement** | Moderate | Moderate | Moderate | Moderate (target-specific) | **P2** |
| **B2. Slot exhaustion** | Easy (social pressure) | Easy | Hard (looks legitimate) | Low-Moderate | **P3** |
| **B3. Bridge targeting** | Hard (need to identify) | Moderate | Moderate | High (community-level) | **P1** |
| **C1. Endorsement spam** | Easy | Easy | Hard | Low | **P3** |
| **C2. Decay erosion** | Free (passive) | Free (passive) | N/A (intended behavior) | Moderate (long-term) | **P3** (UX) |
| **D1. Sybil voting bloc** | Very hard (need many) | Moderate (cheaper per) | Hard (need vote analysis) | High | **P1 at scale** |
| **D2. Adjudication capture** | N/A (doesn't exist) | N/A | N/A | Catastrophic | **P0 when built** |

---

## What this review reveals

### Systematic blind spots in our testing

1. **We test graph properties, not attack campaigns.** Every simulation scenario tests a single mechanism against a static topology. Real attacks are multi-phase: infiltrate (A3) → establish trust → create mesh (A4) → attack (B1 or D1). No simulation tests a multi-phase attack.

2. **We assume endorsements are genuine.** The diversity metric proves that fake endorsements (mesh-internal) can't inflate diversity. But it has nothing to say about *purchased* genuine endorsements. A real user who endorses a Sybil for money creates a real, legitimate edge that provides a real independent path.

3. **We have no post-creation defense.** Once an account passes eligibility, there's no ongoing verification. No behavioral monitoring, no periodic re-validation, no anomaly detection. The diversity check is a one-time gate, not continuous assurance.

4. **Account compromise is a complete bypass.** The entire endorsement ceremony, weight table, and diversity threshold are designed to make *new identity creation* expensive. Account takeover skips all of it. The compromised account IS trusted — legitimately, by real people who knew the original owner.

5. **The anchor is a single point of failure.** All trust is relative to one root. No multi-anchor design, no anchor rotation, no anchor compromise detection.

### What changes

- **Heuristic detection moves from "nice-to-have" to "required for trust at scale."** Without it, the cost of A1/A2 drops faster than the value of the attack increases.
- **Account compromise needs its own simulation scenarios.** Model: attacker takes over account with diversity=3, distance=2.0. What can they do? How many Sybils can they bootstrap?
- **Multi-phase attack scenarios need to be added to the simulation framework.** Not just "here's a topology, measure scores" but "here's a sequence of attacker actions, measure cumulative impact."
- **Post-creation monitoring is not optional.** The current design treats eligibility as a gate. It needs to also be a continuous signal — ongoing structural/temporal/behavioral analysis.
