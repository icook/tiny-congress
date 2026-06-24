# Research DAG — Design Brief

> **Status:** Early brainstorm. Not an implementation plan — captures vision and open questions from 2026-03-28 conversation.

## Core Thesis

TinyCongress rooms can host an **autonomous research bot** that users collectively steer and fund. The differentiator vs. existing deep research tools (OpenAI, Gemini, Perplexity) is:

1. **The trace is the product.** Every LLM call, search result, reasoning step, and dollar spent is the artifact users inspect — not a polished summary hiding the machinery.
2. **Research is collaborative.** The bot operates in a room. Multiple people watch it work, comment on findings, and redirect it. No existing tool does this.
3. **Cost is transparent and real.** Credits are 1:1 with dollars. Users see exactly what each research step cost. The platform is a cost pass-through, not an abstraction layer.

## The DAG Model

A research session is a **directed acyclic graph**, not a chat log. Users navigate it spatially (pan/zoom).

### Node Types

| Type | Description |
|------|-------------|
| **Question** | An open research question (proposed, funded, running, or complete) |
| **Claim** | An assertion the bot made, backed by sources |
| **Synthesis** | Where multiple threads converge into a conclusion |
| **Branch point** | Where research forked (bot decision or user steering) |
| **Template** | A mad-libs template for structured question generation (see below) |

### Node States

| State | Meaning |
|-------|---------|
| `proposed` | Someone suggested it, not yet funded |
| `funded` | Enough budget committed to execute |
| `running` | Bot is actively researching |
| `complete` | Research finished, results attached |
| `disputed` | A later finding contradicts this node |
| `archived` | Lost a top-k vote but still visible, revivable |

### Edges

Edges encode *why* one node led to another: "this source contradicted that claim, so I searched for X." Edges are typed (e.g., `led_to`, `contradicts`, `refines`, `synthesizes`).

## Alternating Rounds: Diverge / Converge

The two core activities alternate:

### Round type A: Mad Libs (diverge)

Structured templates where users fill slots to define the question space:

```
"What evidence exists that [___________] affects [___________]
 in the context of [___________]?"

 Slot 1 options:     Slot 2 options:      Slot 3 options:
 ┌─ carbon pricing   ┌─ housing costs     ┌─ mid-size US cities
 ├─ zoning reform    ├─ transit usage     ├─ EU member states
 └─ UBI programs     └─ labor mobility    └─ rural communities
```

Each combination of fills is a potential branch. **Top-k branches** (by sponsorship) get explored; the rest stay visible but greyed out (archived), revivable later.

**Template authorship (for prototype):** The bot generates templates from its own findings. "I found three threads — here are templates to explore each." Human-authored templates and freeform question proposals are future work.

### Round type B: Deep Research (converge)

The bot executes funded question branches using the standard agentic research loop:

```
Plan → multi-hop search → full-page read → self-critique → synthesize → cite
```

Results populate claim and synthesis nodes in the DAG, with full trace data attached.

### How they connect

Mad libs is the **steering layer** (what to ask). Deep research is the **execution layer** (going and finding out). The DAG contains both: template nodes and research nodes. New templates emerge from research findings, creating the next diverge round.

> **Open question:** The mad libs idea and deep research are somewhat separate mechanisms. The alternating-rounds model is one way to connect them. There may be a tighter integration — or they may be better as two independent room activities. Needs prototyping.

## Credit / Sponsorship Model

### Credits = Dollars

No abstraction layer. 1 credit = $0.01 USD (or similar small denomination). Common sponsorship amounts: $0.01–$0.50.

### Sponsorship Semantics

A sponsorship is a **maximum willingness to spend**, not a fixed price:

> "I'm willing to spend up to $0.50 to get this question answered."

Multiple sponsors can pool toward a question. The bot estimates cost before executing. If pooled commitments cover the estimate, the branch runs. Actual cost is split proportionally among sponsors.

### Cost Structure

**Search** — discrete, predictable. Fixed-price tiers:

```
Shallow (3 searches)   — ~$0.03
Standard (10 searches) — ~$0.10
Deep (25 searches)     — ~$0.25
```

**LLM processing** — continuous, variable. Input cost depends on source article size (uncontrollable). Output cost controllable via structured templates and max length. Metered against remaining budget in real time.

### Budget Lifecycle

1. Sponsors commit max budget to a question node
2. Bot estimates cost and plans within budget
3. Search phase: fixed cost, deducted upfront
4. Processing phase: metered, draws against remainder per article/synthesis call
5. If budget exhausted mid-research: bot stops and reports partial findings, optionally requesting extension ("I found something interesting but need ~$0.15 more")
6. Unspent budget: returned or held for follow-up (TBD)

### Live Cost Accounting (per node)

```
┌─────────────────────────────────────────┐
│  ● How does zoning reform affect...     │
│                                         │
│  Budget: $0.50 (3 sponsors)             │
│  Spent:  $0.33                          │
│    ├ 10 searches      $0.10             │
│    ├ 6 articles read   $0.18            │
│    └ 2 synthesis calls $0.05            │
│  Remaining: $0.17                       │
│                                         │
│  [Continue deeper]  [Refund remainder]  │
└─────────────────────────────────────────┘
```

Every cent accounted for. The trace is both intellectual provenance and financial receipt.

## DAG Visualization

The active path stays small and manageable. Three visual tiers:

- **Active** — currently funded/running/recently completed. Full detail.
- **Proposed** — in the current round, collecting sponsorship. Visible, interactive.
- **Archived** — lost a previous top-k vote or from expired rounds. Greyed out, revivable with new sponsorship.

Top-k pruning at each branch point prevents combinatorial explosion. The unpursued paths remain visible ("things we could know but haven't paid to find out").

## Challenges and Contradictions

Research should surface disagreement transparently:

- Claim nodes can be marked `disputed` when a later finding contradicts them
- Users should be able to sponsor **challenges** to existing claims (essentially funding adversarial research)
- Partial contradictions shown alongside the original claim, not hidden

> **Open question:** Mechanics for how disputes are initiated and resolved. Does the bot self-identify contradictions? Can users flag a claim for re-examination? Both?

## Open Questions

1. **Template authorship beyond prototype.** Bot-generated templates are fine for V1. Eventually: user-proposed templates, room-creator-seeded templates, freeform question escape hatches. How do freeform questions get slotted into the graph structure?

2. **When are sponsorships open/closed?** A question node needs to collect sponsorship before executing. Is there a time window? Does it close when execution starts? Can people add budget mid-execution?

3. **Platform margin.** Is this pure cost pass-through or does the platform take a margin? Business model question — architecturally, support either.

4. **Dispute mechanics.** How are contradictions surfaced? Bot self-detection vs. user-initiated challenges vs. both.

5. **Mad libs + deep research integration tightness.** The alternating-rounds model connects them temporally. Is there a tighter structural integration, or are they better as independent room activities that happen to feed each other?

6. **Scale of the DAG.** How many active nodes before the visualization becomes unusable? What's the right default zoom level?

## Relationship to Existing Work

- `.plan/2026-03-19-steerable-research.md` — Implementation plan for a simpler version (FIFO suggestion queue, basic processing). That work is a stepping stone; the queue mechanics and content filter trait are reusable. The DAG model described here is the evolution.
- `#793` (room bot infrastructure) — Prerequisite. Bot task dispatch, pgmq, trace infrastructure.
- `#743` (per-room autonomous bots) — Architectural alignment. Each room's research bot is an instance of the pattern described here.

## Landscape Context

Survey of existing deep research tools (2026-03-28):

- **OpenAI Deep Research**: Plan → iterative search (dozens of rounds) → full-page read → synthesis. 5-30 min. No mid-task steering. Trace is a collapsible debug panel, not the product.
- **Gemini Deep Research**: Editable outline before execution (best pre-steering). 5-15 min. Google Search integration. No mid-task steering once started.
- **Perplexity Deep Research**: Best citation UX (inline + sidebar). 3-10 min. Steering via follow-up questions, not mid-task.
- **Open source (GPT-Researcher, LangGraph)**: Same plan→search→critique→synthesize loop. Tavily for search. Configurable but not collaborative.

**Common pattern across all:** Plan-then-execute, multi-hop retrieval, full-page reading, self-critique, citation as first-class output. **None are collaborative. None expose cost. None make the trace the primary artifact.**
