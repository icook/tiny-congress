# Brand Ethicality Room — Design

**Date:** 2026-03-17
**Status:** Approved
**Approach:** Extend sim binary (Approach A)

## Concept

A single "Brand Ethics" room where each poll is a different S&P 500 company. Users evaluate companies on 5 fixed ethical dimensions with LLM-generated evidence cards providing context. The room rotates companies on a 24h cadence in a ring buffer — when the last company closes, the first reactivates with fresh evidence. The room never goes idle.

The value proposition: most people have never thought about the ethics of companies that deeply affect their daily lives. "You've never heard of Sysco but they handle 60% of the food in your kid's school cafeteria" is more compelling than another take on Apple.

## The Room

- **Name:** "Brand Ethics"
- **Constraint:** `identity_verified` (requires #709)
- **Rotation:** `poll_duration_secs = 86400` (24h per company)
- **Lifecycle:** Ring buffer — when all companies have been evaluated, reset polls to draft, regenerate evidence, restart the cycle

Each poll represents one company:
- `question`: Company name (e.g., "Sysco Corporation")
- `description`: LLM-generated relevance hook — "How this company touches your life" (2-3 sentences)
- 5 fixed dimensions with evidence cards

## Dimensions

Fixed across all companies for cross-company comparability:

| Dimension | Min Label | Max Label |
|---|---|---|
| Labor Practices | Exploitative | Exemplary |
| Environmental Impact | Destructive | Regenerative |
| Consumer Trust | Deceptive | Transparent |
| Community Impact | Extractive | Invested |
| Corporate Governance | Self-Serving | Accountable |

## Evidence Cards

Per-dimension pro/con claims providing factual context for informed voting. Typically 1-2 pro and 1-2 con per dimension (~10-20 per company).

**Data model — new table:**

```sql
CREATE TABLE rooms__poll_evidence (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    dimension_id UUID NOT NULL REFERENCES rooms__poll_dimensions(id) ON DELETE CASCADE,
    stance       TEXT NOT NULL CHECK (stance IN ('pro', 'con')),
    claim        TEXT NOT NULL,
    source       TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_poll_evidence_dimension ON rooms__poll_evidence(dimension_id);
```

- `stance`: "pro" or "con" relative to the dimension's positive end
- `claim`: One sentence, factual tone, LLM-generated
- `source`: Optional attribution string (not verified URLs)

**API:** Evidence is inlined in the existing `GET /rooms/:roomId/polls/:pollId` response, nested under each dimension. No separate endpoint.

## LLM Content Generation

Two-phase generation via existing OpenRouter client (`sim/llm.rs`):

### Phase 1: Company Curation (one-time at seed)

Prompt the LLM with the S&P 500 list. Ask it to select and rank ~25 companies by "surprising personal relevance" — companies that touch daily life but have low brand awareness. Deprioritize household tech/retail names.

Output: ordered list of `(ticker, company_name, one_line_hook)`. Becomes the poll agenda order.

### Phase 2: Per-Company Content (eager at seed)

For each company, prompt with company name + the 5 dimension definitions. Generate:
- `relevance_hook`: 2-3 sentences on daily-life connection (becomes poll description)
- Per dimension: 1-2 pro claims, 1-2 con claims (become evidence rows)

All content generated eagerly at seed time — no runtime OpenRouter dependency.

**Cost:** ~25 LLM calls at ~1K tokens each. Under $1 total.

## Sim Binary Changes

**New config vars:**

| Var | Default | Purpose |
|---|---|---|
| `SIM_ROOM_TOPIC` | `civic` | `civic` (current behavior) or `brand_ethics` |
| `SIM_COMPANY_COUNT` | `25` | Companies to curate from S&P 500 |
| `SIM_POLL_DURATION_SECS` | `86400` | Per-company rotation cadence |

**Flow when `SIM_ROOM_TOPIC=brand_ethics`:**

1. Identity bootstrap — unchanged (verifier + voters)
2. Check if "Brand Ethics" room exists. If not:
   - Phase 1: curate N companies
   - Create room with `constraint_type = identity_verified`
   - Per company: create draft poll, create 5 fixed dimensions, Phase 2 LLM call, insert evidence rows
   - First poll auto-activates via lifecycle queue
3. Ring buffer refill — if room has no active/draft polls, reset all polls to draft, delete old evidence, regenerate via Phase 2, re-activate first poll
4. Vote seeding — unchanged

**`civic` mode is the default and unchanged.** `brand_ethics` is a parallel path selected by config.

**Local invocation:**

```bash
export DATABASE_URL="postgres://..."
export SIM_BASE_URL="http://localhost:3000"
export SIM_OPENROUTER_API_KEY="sk-or-..."
export SIM_ROOM_TOPIC="brand_ethics"
export SIM_COMPANY_COUNT=5
cargo run --bin sim
```

## Frontend Changes

**Poll page (`Poll.page.tsx`):**

1. **Relevance hook** — already renders via poll `description` field. No change needed.
2. **Evidence cards** — new component per dimension, between label and slider:
   - Pro claims with subtle green indicator, con with red/orange
   - Collapsed by default on mobile (tap to expand)
   - Mantine `Accordion` or `Collapse` component
3. **Company name as heading** — poll `question` field. Already renders. Minor styling to feel like a company evaluation.
4. **No new pages or routes.**

## Ring Buffer Behavior

When capacity-fill detects the room has no active/draft polls:

1. Reset all closed polls to `draft` status
2. Delete old evidence rows
3. Re-run Phase 2 LLM calls per company (fresh claims)
4. Lifecycle queue activates the first poll

Votes from previous cycles persist — cumulative results across cycles. Cycle-scoped results are a future enhancement.

## Change Summary

| Component | Change |
|---|---|
| Migration | `rooms__poll_evidence` table |
| Sim config | `SIM_ROOM_TOPIC`, `SIM_COMPANY_COUNT`, `SIM_POLL_DURATION_SECS` |
| Sim logic | Brand ethics mode: curation, fixed dimensions, evidence gen, ring buffer |
| LLM prompts | Phase 1 (company curation), Phase 2 (per-company evidence) |
| Backend API | Extend poll response with evidence per dimension |
| Frontend | Evidence cards component (collapsible on mobile) |

## Dependencies

- **#709 (constraint refactor)** — needed for `identity_verified` constraint on the room. Can develop in parallel using `endorsed_by` as a temporary constraint.
