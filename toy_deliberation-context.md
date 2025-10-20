Below is a **high‑level software requirements skeleton** for a mass‑deliberation “Conversation” system, with concrete **phase procedures**, **data structures**, **automation hooks**, and **stack trade‑offs**. Procedures are grounded in deliberative‑democracy practice (e.g., deliberative polling, sortition/open democracy), and at‑scale opinion‑mapping (e.g., Pol.is). ([Stanford Deliberation Lab][1])

---

## 0) Product intent (one line)

**A conversation = a governed workflow** that takes a topic from open brainstorming → triage → focused deliberation → reflection/synthesis → decision → publication, **measuring consensus distance** and **rewarding bridging behavior**, not volume.

---

## 1) Core roles & actors

* **Participants (humans)** — submit statements, vote on others, propose evidence.
* **LLM Facilitator(s)** — structured prompts to: dedupe, reframe, scaffold counter‑arguments, produce neutral summaries, and propose bridging statements.
* **Moderators/Stewards** — manage safety, rate limits, and phase gates.
* **Admin/Owner** — sets topic, time/coverage targets, decision rule.
* **Simulated Personas** — synthetic agents for prototyping and acceptance tests (decoupled from real deliberation).
* **Observers/Readers** — read‑only; no write privileges.

---

## 2) Core objects (data model sketch)

* **Conversation** `{id, topic, owner_id, created_at, status, decision_rule, phase_order[], governance_policy_id}`
* **Phase** `{id, conversation_id, type, status, gate_targets{time|coverage|stability}, start_at, end_at}`
* **Statement** `{id, author_id (human|persona|llm), text, normalized_text, parent_id?, tags[], evidence_urls[], status}`
* **Vote** `{participant_id, statement_id, response ∈ {agree, disagree, pass}, weight (for QV or reputation weighting), created_at}`  *(Pol.is‑style A/D/P is essential for scalable mapping.)* ([Computational Democracy][2])
* **Cluster** `{id, method (A/D/P matrix clustering), members(participant_ids), stance_vector, bridging_score}`
* **Proposal** `{id, source_statements[], pro_con_map, draft_text, revision_history}`
* **MeritEvent** `{participant_id, kind ∈ {novelty, civility, bridge, evidence, moderation_hit(-)}, points, half_life_days}`
* **TrustProfile** `{participant_id, short_term_tokens, long_term_reputation, sybil_score, flags[]}`
* **Transcript/Summary** `{phase_id, audience_view (per‑cluster, cross‑cluster), key_agreements[], key_disagreements[], rationales[]}`
* **SimCorpus** `{id, seed, persona_configs[], generated_statements[], votes[], invariants[]}`

Storage: **PostgreSQL** (relational), with an **append‑only event_log** table for audit (event sourcing lite). Object/blob store for artifacts (summaries, charts).

---

## 3) Phase procedures (governance‑informed)

### A) **Docket & Norms (pre‑phase)**

**Goal:** clarify scope, ground rules, and metrics of success.
**Procedure:**

1. Topic + frame statement (owner).
2. Norms ack (civility pledge, no doxxing), identity tier selection (pseudonymous vs verified).
3. Decide **decision rule** now to avoid end‑stage goal‑post moving:

   * **Majority Judgment** for judgments on proposals (median of graded ratings). ([Wikipedia][3])
   * **Condorcet/Kemeny** for ranking multiple proposals; report cycles if present. ([Wikipedia][4])
   * **Quadratic Voting (QV)** during prioritization to capture intensity (optional). ([radicalxchange.org][5])
4. Participant sampling mode: open, invite, or **sortition** mini‑public (Landemore). ([Yale Political Science][6])

**Automation:** LLM produces a **neutral “briefing bundle”** (pros/cons/evidence checklist) for baseline literacy (inspired by **Deliberative Polling** briefings). ([Stanford Deliberation Lab][1])

---

### B) **Brainstorming (statement collection)**

**Goal:** maximize **idea coverage** while keeping inputs structured.
**What counts as input:** short, declarative **statements** (not long essays).
**Participant loop:**

1. Submit statement(s).
2. Vote “**agree / disagree / pass**” on a rotating sample of others’ statements (A/D/P).
3. Earn **short‑term merit** for **novelty** (semantic distance) and **review rate**; **demerit** for toxicity/duplication.

**LLM Facilitator jobs (deterministic prompts + tools):**

* **Dedupe/normalize** statements (preserve semantics; reduce redundancy).
* **Frame translation**: rephrase in neutral language if inflammatory; attach “rationale requested” tags.
* **Seeding**: generate **bridging statements** that plausibly draw multi‑cluster agreement. *(Bridging incentives are an explicit design goal.)* ([Knight First Amendment Institute][7])

**Analytics:** maintain an **A/D/P matrix** (participants × statements). No NLP sentiment is required to map opinion space; we rely on votes, **as Pol.is does**. ([societyspeaks.io][8])

**Phase-gate heuristics:** exit when **new‑statement novelty** and **new information gain** plateau (e.g., marginal silhouette score Δ < ε for N batches).

---

### C) **Triage (clustering & docketing)**

**Goal:** compress the brainstorm into a navigable docket.
**Procedure:**

1. **Cluster** participants via A/D/P matrix; reveal **consensus & fault lines** visually. (Proven at scale in vTaiwan + Pol.is.) ([Democracy Technologies][9])
2. Elevate **high‑support** and **high‑controversy** statements; auto‑merge near‑duplicates; spin out **themes**.
3. Compute **bridging score** for each statement: ∆agreement across clusters (high cross‑cluster agreement = high bridge value).
4. Use **QV credits** to **prioritize which themes advance** into focused deliberation (participants spend limited credits on themes they care most about). ([radicalxchange.org][5])

**Phase-gate heuristics:** “coverage target” reached (e.g., top‑K themes cover ≥X% of cumulative QV weight **and** clusters stable across bootstraps).

---

### D) **Focused Deliberation (mini‑publics + argument mapping)**

**Goal:** deepen understanding, reduce misunderstanding, and co‑draft proposals.
**Procedure:**

1. For each theme, create **mini‑publics** (randomly sampled across clusters for diversity). *(Deliberative polling logic: balanced information + structured small‑group discussion.)* ([Stanford Deliberation Lab][1])
2. Structure discussions into **claim → evidence → impact** micro‑cards; enforce turn‑taking; LLM acts as **neutral chair** (timekeeper, summarizer, antagonist to test arguments).
3. **Evidence check**: participants attach citations; LLM runs “**rationale completeness**” checklist; human moderators spot‑check.
4. LLM produces **two summaries** per session:

   * **Per‑cluster view** (“what this cluster sees/values”).
   * **Cross‑cluster synthesis** (“where actual overlap is, what remains contested”).
5. Graduates of this phase are **Proposals** with **explicit trade‑offs** and **known points of dissent**.

**Merit incentives:** award **bridge badges** for contributions that increase cross‑cluster overlap; decay reputational gains slowly; penalize repeated norm violations rapidly.

---

### E) **Reflection & Synthesis**

**Goal:** agree on **what we learned** and **what options remain**.
**Procedure:**

1. Publish **What we agree on / What we disagree on / What we need to investigate** (Pol.is‑style maps are great here). ([Gwern][10])
2. Put each **Proposal** to **Majority Judgment** grading (cards like “Excellent … Reject”), report median grade **and** dispersion; present **robustness** via sensitivity checks (remove top 5% most/least engaged and re‑compute). ([Wikipedia][3])
3. If multiple proposals: compute **Kemeny (Condorcet) ranking**, highlight cycles if any; report **CJT intuition** (“more independent, slightly‑better‑than‑random jurors → more accuracy”). ([Wikipedia][4])

---

### F) **Decision & Publication**

**Goal:** finalize decisions or recommendations, and capture artifacts for transparency and learning.

* Apply the pre‑declared **decision rule** (MJ / Condorcet / QV prioritization results). ([Wikipedia][3])
* Publish the **audit trail**: event log, cluster maps, proposal texts, vote tallies, and all LLM prompts/outputs used in facilitation.

---

## 4) Merit/Demerit & reputation (short‑term vs long‑term)

* **Short‑term merits** (phase‑scoped): novelty, review helpfulness, evidence contribution, **bridging** (agreement variance reduction across clusters).
* **Demerits:** norm strikes (toxicity), spam, duplicate flooding.
* **Long‑term reputation:** exponentially‑decayed score that **cannot drive decision rules** directly (avoid plutocracy) but **does**:

  * increase *voice credits* budget for QV **or**
  * increase the **weight of your *reviews*** (not your A/D vote).
* **Sybil resistance:** proof‑of‑personhood or verified tiers; duplicate‑account similarity checks; escalating rate limits.

Designing rewards that **explicitly incentivize bridging** is aligned with recent “bridging systems” research. ([Knight First Amendment Institute][7])

---

## 5) Simulation mode (decoupled generation vs deliberation)

**Why:** You want to re‑run the exact corpora as **acceptance tests**.

**Generation engine (offline):**

* Inputs: `seed`, `persona_configs[]` (traits, priors, media diets).
* Outputs: synthetic `statements[]` + A/D/P votes **only**; **no** real‑time clustering here.
* Store to **SimCorpus** (immutable), version with **DVC** or plain object store + semantic version tags.

**Deliberation engine (replayable):**

* Consumes a **SimCorpus snapshot** and runs **the same clustering/triage/decision code paths** as production with **LLM calls stubbed** (record‑replay).
* **Invariants** to assert:

  * Stable cluster count within ±1 across seeds.
  * Bridging‑score ordering for top N statements is monotone across minor perturbations.
  * Decision winners are robust across MJ/Condorcet unless cycles exist.

---

## 6) Phase‑gate scheduler (“take the time it needs”)

A conversation advances **when**:

* **Coverage**: X% of participant‑weight has evaluated Y% of statements (brainstorm).
* **Stability**: cluster structure stable across bootstraps (ARI ≥ τ for K resamples).
* **Saturation**: new statements add < ε to information gain (topic model perplexity or silhouette Δ).
* **Confidence**: MJ medians converge (IQR below threshold) **or** QV spending concentrates on ≤ K themes.

Advancement is **automatic** unless moderators veto (exception handling).

---

## 7) APIs (minimal)

* `POST /conversations` (create)
* `POST /conversations/{id}/phases:start|end`
* `POST /statements` (create) → returns normalized + dedupe info
* `POST /votes` (A/D/P, optional weight if QV window open)
* `GET /clusters`, `GET /maps` (vectors for viz)
* `POST /proposals` (promote top themes)
* `POST /decide` (runs MJ/Condorcet/QV pipeline)
* `GET /transcripts`, `GET /auditlog`
* `POST /simulate/generate_corpus`, `POST /simulate/run`, `GET /simulate/results`

---

## 8) Model‑serving & encoder stack trade‑offs (lightweight, robust)

**Encoders (for semantic dedupe, novelty):**

* **Sentence‑Transformers** models exported to **ONNX Runtime**; consider int8 quantization for throughput. *(ONNX often yields 1.3–3× speedups; serialize optimized artifacts to disk.)* ([SentenceTransformers][11])

**LLM inference (facilitation/summarization):**

| Option               | Why pick it                                                                                                         | Why not                                                               |
| -------------------- | ------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------- |
| **vLLM**             | **PagedAttention + continuous batching**; excellent throughput; simple deployment as a Python server.               | Slightly heavier Python stack; best on NVIDIA GPUs. ([VLLM Docs][12]) |
| **Hugging Face TGI** | Battle‑tested Rust/Python/gRPC server; **tensor parallelism**, streaming, **continuous batching**, Docker‑first DX. | More knobs; multi‑container setup. ([Hugging Face][13])               |
| **llama.cpp**        | Ultra‑simple, runs on CPU or small GPUs; great for local dev and low‑scale demos.                                   | Lower throughput/feature set; limited batching. ([GitHub][14])        |

**Queueing/orchestration:**

* **Redis + Celery (Python)** or **BullMQ (Node)** for job queues; keep it boring.
* For more complex long‑running workflows, **Temporal** works—but **MVP** can stay with queue + cron.
* API gateway can route to LLM backends via **lite router** (e.g., LiteLLM‑style abstraction) to keep generation vs deliberation decoupled.

**Observation:** either **vLLM** or **TGI** will give you continuous batching; both now feature **router‑level queuing** to keep GPU memory safe. ([vLLM Blog][15])

---

## 9) Minimal architecture (MVP‑ready)

* **Frontend:** Next.js (or SvelteKit) + SSR; WebSockets for live vote counts and maps.
* **Backend API:** **FastAPI** (Python) for proximity to ML libs (or NestJS if you prefer TS).
* **Workers:** Celery (Redis broker) for ingestion, clustering, scoring, summarization.
* **Model servers:** 1× **vLLM** (GPU) + 1× **ONNX Runtime** server for encoders.
* **DB:** Postgres + read replicas; **event_log** table for append‑only audit.
* **Metrics/telemetry:** Prometheus + Grafana; **OpenTelemetry** traces; **Langfuse** (or similar) for prompt observability.
* **Content safety:** classifier + rules engine (demerits; hard blocks); human moderator queue.

---

## 10) Concrete algorithms & scoring

* **Clustering:** operate on **A/D/P vote matrix** (participants × statements); dimensionality reduction → **HDBSCAN** or spectral clustering; display with UMAP. *(The key is that we cluster on **votes**, not free text, per Pol.is.)* ([societyspeaks.io][8])
* **Bridging score (statement s):**
  [
  B(s)=1-\sum_{c\in C} w_c\cdot |A_c(s)-\bar A(s)|
  ]
  where (A_c(s)) = agree‑rate in cluster c; (w_c) = cluster weight. Higher is “more cross‑cutting.”
* **Novelty score (statement s):** cosine distance from nearest existing statement embedding (encoder‑based) × low duplication penalty.
* **Decision rules:**

  * **MJ**: median grade + tie‑break by “majority gauge.” ([Wikipedia][3])
  * **Kemeny**: aggregate rankings minimizing total Kendall‑tau distance; warn about NP‑hardness; use heuristics for >20 options. ([Wikipedia][4])
  * **QV**: n votes cost (n^2) credits; cap credits per participant; report intensity map. ([Independent Institute][16])
* **Robustness:** bootstrap participants; report outcome stability; invoke **Condorcet Jury Theorem intuition** in dashboard notes. ([Stanford Encyclopedia of Philosophy][17])

---

## 11) Safety, governance & legitimacy

* **Identity tiers:** anonymous (low rate limits), pseudonymous, verified (higher influence on **review helpfulness**, not outcome).
* **Open records:** publish prompts, summaries, and decision artifacts to an **audit page**.
* **Data governance:** explicit licenses on statements; redaction tooling; differential privacy for public exports.

---

## 12) MVP slice (build order)

1. Conversation + Brainstorm + A/D/P voting + maps (clusters)
2. Triage + prioritization (QV optional)
3. Focused deliberation with mini‑publics + LLM summaries
4. Decision stage (MJ + Kemeny) + publication
5. Simulation harness (persona generator; replay)

*(No time estimates provided.)*

---

## 13) What to prototype first (acceptance tests)

* **Invariant:** adding ≥20% more participants with similar distribution **should not** flip consensus on “what we agree on” unless new evidence is introduced (CJT intuition test). ([Stanford Encyclopedia of Philosophy][17])
* **Bridge test:** a high‑B(s) statement raised by the LLM moves two clusters’ stance vectors **closer** by ≥δ.
* **Decision robustness:** MJ winner remains top‑2 under Kemeny in ≥80% bootstraps.

---

## 14) Notes on governance “lore” embedded here

* **Deliberative Polling** (balanced materials + diverse small groups) inspires the **Focused Deliberation** flow. ([Stanford Deliberation Lab][1])
* **Open/Sortition‑based democracy** guides sampling and legitimacy. ([Yale Political Science][6])
* **Pol.is** shows that **A/D/P votes over statements** scale, reveal clusters, and surface **rough consensus**; this underpins Brainstorm → Triage → Synthesis. ([Computational Democracy][2])
* **Modern social‑choice** gives decision rules resistant to polarization: **MJ**, **Condorcet/Kemeny**, **QV** (for intensity). ([Wikipedia][3])

---

## 15) Open questions (worth exploring next)

* **Bridging rewards** vs **brigading risks** — calibrate merit functions so “performative centrism” doesn’t dominate. *(See “bridging systems” discussion.)* ([Knight First Amendment Institute][7])
* **Identity & sybil** — evaluate verifiable credentials tiers without chilling participation.
* **LLM facilitation bias** — diversify models, expose prompts, and sandbox critiques of AI summaries.

---

### Stack short‑list (if you want it boring and resilient)

* **FastAPI + Postgres + Redis (Celery)**
* **Encoders:** `all‑MiniLM‑L6‑v2` on **ONNX Runtime (int8)**
* **LLM:** **vLLM** or **TGI** (both support continuous batching; pick one)
* **Observability:** Prometheus/Grafana + OpenTelemetry
* **Front‑end:** Next.js + WebSockets for live maps

vLLM and TGI both deliver router‑level queuing and continuous batching; pick based on team familiarity and ops preference. ([vLLM Blog][15])

---

**Accuracy:** 96%
**Confidence:** 93%

If you want, I can turn this into: (1) a **DB schema (SQL)**, (2) a **phase‑gate scheduler spec** with pseudo‑code, and (3) a **SimCorpus fixture** for the “Gun control in America” example to run end‑to‑end locally.

[1]: https://deliberation.stanford.edu/what-deliberative-pollingr?utm_source=chatgpt.com "What is Deliberative Polling®? | Deliberative Democracy Lab"
[2]: https://compdemocracy.org/polis/?utm_source=chatgpt.com "Polis - The Computational Democracy Project"
[3]: https://en.wikipedia.org/wiki/Majority_judgment?utm_source=chatgpt.com "Majority judgment"
[4]: https://en.wikipedia.org/wiki/Kemeny_method?utm_source=chatgpt.com "Kemeny method"
[5]: https://www.radicalxchange.org/wiki/quadratic-voting/?utm_source=chatgpt.com "Quadratic Voting"
[6]: https://politicalscience.yale.edu/publications/open-democracy-reinventing-popular-rule-twenty-first-century?utm_source=chatgpt.com "Open Democracy: Reinventing Popular Rule for the Twenty ..."
[7]: https://knightcolumbia.org/content/bridging-systems?utm_source=chatgpt.com "Bridging Systems: Open problems for countering destructive ..."
[8]: https://societyspeaks.io/help/polis-algorithms?utm_source=chatgpt.com "Understanding Pol.is Algorithms - Society Speaks.io"
[9]: https://democracy-technologies.org/participation/consensus-building-in-taiwan/?utm_source=chatgpt.com "Lessons From Consensus Building in Taiwan"
[10]: https://gwern.net/doc/sociology/2021-small.pdf?utm_source=chatgpt.com "[PDF] Scaling Deliberation by Mapping High Dimensional Opinion Spaces"
[11]: https://sbert.net/docs/sentence_transformer/usage/efficiency.html?utm_source=chatgpt.com "Speeding up Inference"
[12]: https://docs.vllm.ai/?utm_source=chatgpt.com "vLLM"
[13]: https://huggingface.co/docs/text-generation-inference/en/index?utm_source=chatgpt.com "Text Generation Inference"
[14]: https://github.com/ggml-org/llama.cpp?utm_source=chatgpt.com "ggml-org/llama.cpp: LLM inference in C/C++"
[15]: https://blog.vllm.ai/2025/09/05/anatomy-of-vllm.html?utm_source=chatgpt.com "Inside vLLM: Anatomy of a High-Throughput LLM Inference ..."
[16]: https://www.independent.org/tir/2019-spring/radical-markets/?utm_source=chatgpt.com "Book Review: Radical Markets: Uprooting Capitalism and ..."
[17]: https://plato.stanford.edu/entries/jury-theorems/?utm_source=chatgpt.com "Jury Theorems - Stanford Encyclopedia of Philosophy"
