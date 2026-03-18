# Brand Ethicality Room Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a "Brand Ethics" room where S&P 500 companies rotate daily, users rate them on 5 ethical dimensions with LLM-generated evidence cards providing pro/con context.

**Architecture:** Extend the existing sim binary with a `brand_ethics` content mode. New `rooms__poll_evidence` table stores per-dimension evidence cards. The poll detail API response is extended to include evidence inline. Frontend adds a collapsible evidence card component per dimension. Ring buffer rotation resets polls when the company queue is exhausted.

**Tech Stack:** Rust (sqlx, axum), React (Mantine, TanStack Query), OpenRouter LLM API

---

### Task 1: Migration — `rooms__poll_evidence` table

**Files:**
- Create: `service/migrations/19_poll_evidence.sql`

**Step 1: Write the migration**

```sql
-- 19_poll_evidence.sql
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

**Step 2: Verify migration number is still available**

Run: `ls service/migrations/*.sql | sort -V | tail -1`
Expected: `18_invite_weight_fields.sql` — confirming 19 is next. If not, renumber.

**Step 3: Run backend tests to verify migration applies**

Run: `cd service && cargo test --test db_tests -- --nocapture 2>&1 | tail -20`
Expected: Tests pass (testcontainers applies all migrations including the new one).

**Step 4: Commit**

```bash
git add service/migrations/19_poll_evidence.sql
git commit -m "feat(rooms): add rooms__poll_evidence table for per-dimension evidence cards"
```

---

### Task 2: Backend repo — evidence insert and query

**Files:**
- Create: `service/src/rooms/repo/evidence.rs`
- Modify: `service/src/rooms/repo/mod.rs` (add `pub mod evidence;`)

**Step 1: Write the failing test**

Add to `service/tests/rooms_tests.rs` (or create `service/tests/evidence_tests.rs` if rooms_tests doesn't exist — check first):

```rust
#[tokio::test]
async fn test_insert_and_get_evidence() {
    let pool = test_pool().await;
    // Create a room, poll, dimension first via existing repo functions
    let room = rooms::create_room(&pool, "Evidence Test Room", None, "test", "endorsed_by", &serde_json::json!({}), None).await.unwrap();
    let poll = polls::create_poll(&pool, room.id, "Test Poll", None).await.unwrap();
    let dim = polls::add_dimension(&pool, poll.id, "Test Dim", None, 0.0, 1.0, 0, None, None).await.unwrap();

    // Insert evidence
    let rows = evidence::insert_evidence(
        &pool,
        dim.id,
        &[
            evidence::NewEvidence { stance: "pro", claim: "Good thing happened", source: Some("Reuters") },
            evidence::NewEvidence { stance: "con", claim: "Bad thing happened", source: None },
        ],
    ).await.unwrap();
    assert_eq!(rows, 2);

    // Query evidence by dimension IDs
    let results = evidence::get_evidence_for_dimensions(&pool, &[dim.id]).await.unwrap();
    assert_eq!(results.len(), 2);
    assert!(results.iter().any(|e| e.stance == "pro" && e.claim == "Good thing happened"));
    assert!(results.iter().any(|e| e.stance == "con" && e.source.is_none()));
}

#[tokio::test]
async fn test_delete_evidence_for_poll() {
    let pool = test_pool().await;
    let room = rooms::create_room(&pool, "Delete Evidence Room", None, "test", "endorsed_by", &serde_json::json!({}), None).await.unwrap();
    let poll = polls::create_poll(&pool, room.id, "Test Poll", None).await.unwrap();
    let dim = polls::add_dimension(&pool, poll.id, "Dim", None, 0.0, 1.0, 0, None, None).await.unwrap();

    evidence::insert_evidence(&pool, dim.id, &[
        evidence::NewEvidence { stance: "pro", claim: "Claim", source: None },
    ]).await.unwrap();

    let deleted = evidence::delete_evidence_for_poll(&pool, poll.id).await.unwrap();
    assert_eq!(deleted, 1);

    let remaining = evidence::get_evidence_for_dimensions(&pool, &[dim.id]).await.unwrap();
    assert!(remaining.is_empty());
}
```

**Step 2: Run test to verify it fails**

Run: `cd service && cargo test --test evidence_tests -- --nocapture 2>&1 | tail -10`
Expected: Compilation error — `evidence` module doesn't exist.

**Step 3: Implement the repo module**

Create `service/src/rooms/repo/evidence.rs`:

```rust
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, sqlx::FromRow)]
pub struct EvidenceRecord {
    pub id: Uuid,
    pub dimension_id: Uuid,
    pub stance: String,
    pub claim: String,
    pub source: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub struct NewEvidence<'a> {
    pub stance: &'a str,
    pub claim: &'a str,
    pub source: Option<&'a str>,
}

/// Insert multiple evidence rows for a single dimension.
/// Returns the number of rows inserted.
pub async fn insert_evidence<'e, E: sqlx::Executor<'e, Database = sqlx::Postgres>>(
    executor: E,
    dimension_id: Uuid,
    evidence: &[NewEvidence<'_>],
) -> Result<u64, sqlx::Error> {
    // Build batch insert with unnest
    let mut stances: Vec<&str> = Vec::with_capacity(evidence.len());
    let mut claims: Vec<&str> = Vec::with_capacity(evidence.len());
    let mut sources: Vec<Option<&str>> = Vec::with_capacity(evidence.len());

    for e in evidence {
        stances.push(e.stance);
        claims.push(e.claim);
        sources.push(e.source);
    }

    let result = sqlx::query(
        r#"
        INSERT INTO rooms__poll_evidence (dimension_id, stance, claim, source)
        SELECT $1, unnest($2::text[]), unnest($3::text[]), unnest($4::text[])
        "#,
    )
    .bind(dimension_id)
    .bind(&stances)
    .bind(&claims)
    .bind(&sources)
    .execute(executor)
    .await?;

    Ok(result.rows_affected())
}

/// Get all evidence records for a set of dimension IDs.
pub async fn get_evidence_for_dimensions<'e, E: sqlx::Executor<'e, Database = sqlx::Postgres>>(
    executor: E,
    dimension_ids: &[Uuid],
) -> Result<Vec<EvidenceRecord>, sqlx::Error> {
    sqlx::query_as::<_, EvidenceRecord>(
        r#"
        SELECT id, dimension_id, stance, claim, source, created_at
        FROM rooms__poll_evidence
        WHERE dimension_id = ANY($1)
        ORDER BY dimension_id, stance DESC, created_at
        "#,
    )
    .bind(dimension_ids)
    .fetch_all(executor)
    .await
}

/// Delete all evidence for dimensions belonging to a specific poll.
/// Used by ring buffer reset. Returns rows deleted.
pub async fn delete_evidence_for_poll<'e, E: sqlx::Executor<'e, Database = sqlx::Postgres>>(
    executor: E,
    poll_id: Uuid,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
        DELETE FROM rooms__poll_evidence
        WHERE dimension_id IN (
            SELECT id FROM rooms__poll_dimensions WHERE poll_id = $1
        )
        "#,
    )
    .bind(poll_id)
    .execute(executor)
    .await?;

    Ok(result.rows_affected())
}
```

Add `pub mod evidence;` to `service/src/rooms/repo/mod.rs`.

**Step 4: Run tests to verify they pass**

Run: `cd service && cargo test --test evidence_tests -- --nocapture 2>&1 | tail -10`
Expected: PASS

**Step 5: Commit**

```bash
git add service/src/rooms/repo/evidence.rs service/src/rooms/repo/mod.rs service/tests/evidence_tests.rs
git commit -m "feat(rooms): add evidence repo with insert, query, and delete"
```

---

### Task 3: Backend API — extend poll detail with evidence

**Files:**
- Modify: `service/src/rooms/http/mod.rs` (~L259, ~L280 — `get_poll_detail` handler and `PollDetailResponse`)
- Modify: `service/src/rooms/service.rs` (~L330 — `get_poll` or add `get_poll_detail` service method)

**Step 1: Write the failing test**

Add an integration test (in the appropriate test file) that hits `GET /rooms/{room_id}/polls/{poll_id}` and asserts the response includes an `evidence` array on each dimension. First seed evidence via the repo, then check the HTTP response shape.

```rust
#[tokio::test]
async fn test_poll_detail_includes_evidence() {
    // Setup: create room, poll, dimension, insert evidence via repo
    // Hit GET /rooms/{room_id}/polls/{poll_id}
    // Assert response.dimensions[0].evidence is a non-empty array
    // Assert each evidence item has: id, stance, claim, source (nullable)
}
```

**Step 2: Run to verify it fails**

Expected: Response does not include `evidence` field on dimensions.

**Step 3: Extend the response types**

In `service/src/rooms/http/mod.rs`, add to the `DimensionResponse` struct (or create a new `DimensionDetailResponse` if `DimensionResponse` is reused elsewhere):

```rust
#[derive(Serialize)]
struct EvidenceResponse {
    id: String,
    stance: String,
    claim: String,
    source: Option<String>,
}
```

Add `evidence: Vec<EvidenceResponse>` to the dimension response within `PollDetailResponse`.

**Step 4: Update the handler**

In `get_poll_detail` handler (~L259):
1. After fetching poll + dimensions, collect all `dimension_ids`
2. Call `evidence::get_evidence_for_dimensions(&pool, &dimension_ids)`
3. Group evidence by `dimension_id` into a `HashMap<Uuid, Vec<EvidenceResponse>>`
4. When building the response, attach each dimension's evidence

**Step 5: Run test to verify it passes**

Run the integration test.
Expected: PASS — evidence appears in response.

**Step 6: Commit**

```bash
git add service/src/rooms/http/mod.rs service/src/rooms/service.rs
git commit -m "feat(api): include evidence cards in poll detail response"
```

---

### Task 4: Frontend types and evidence component

**Files:**
- Modify: `web/src/features/rooms/api/client.ts` (~L30 — `Dimension` type)
- Create: `web/src/features/rooms/components/EvidenceCards.tsx`

**Step 1: Extend the `Dimension` type**

In `web/src/features/rooms/api/client.ts`, add to the `Dimension` interface:

```typescript
export interface Evidence {
  id: string;
  stance: 'pro' | 'con';
  claim: string;
  source: string | null;
}

// Add to Dimension interface:
export interface Dimension {
  // ... existing fields ...
  evidence: Evidence[];
}
```

**Step 2: Write the EvidenceCards component**

Create `web/src/features/rooms/components/EvidenceCards.tsx`:

```tsx
import { Collapse, Group, Stack, Text, UnstyledButton } from '@mantine/core';
import { useDisclosure } from '@mantine/hooks';
import { IconChevronDown, IconChevronRight } from '@tabler/icons-react';

import type { Evidence } from '../api/client';

interface EvidenceCardsProps {
  evidence: Evidence[];
}

export function EvidenceCards({ evidence }: EvidenceCardsProps) {
  const [opened, { toggle }] = useDisclosure(false);

  if (evidence.length === 0) return null;

  const proCards = evidence.filter((e) => e.stance === 'pro');
  const conCards = evidence.filter((e) => e.stance === 'con');

  return (
    <Stack gap={4}>
      <UnstyledButton onClick={toggle}>
        <Group gap={4}>
          {opened ? <IconChevronDown size={14} /> : <IconChevronRight size={14} />}
          <Text size="xs" c="dimmed">
            {evidence.length} evidence {evidence.length === 1 ? 'card' : 'cards'}
          </Text>
        </Group>
      </UnstyledButton>
      <Collapse in={opened}>
        <Stack gap={4} pl="sm">
          {proCards.map((e) => (
            <Group key={e.id} gap={6} wrap="nowrap" align="flex-start">
              <Text c="teal" size="sm" fw={700} style={{ flexShrink: 0 }}>
                +
              </Text>
              <Text size="sm">
                {e.claim}
                {e.source && (
                  <Text span c="dimmed" size="xs">
                    {' '}
                    — {e.source}
                  </Text>
                )}
              </Text>
            </Group>
          ))}
          {conCards.map((e) => (
            <Group key={e.id} gap={6} wrap="nowrap" align="flex-start">
              <Text c="red" size="sm" fw={700} style={{ flexShrink: 0 }}>
                −
              </Text>
              <Text size="sm">
                {e.claim}
                {e.source && (
                  <Text span c="dimmed" size="xs">
                    {' '}
                    — {e.source}
                  </Text>
                )}
              </Text>
            </Group>
          ))}
        </Stack>
      </Collapse>
    </Stack>
  );
}
```

**Step 3: Write a component test**

Create `web/src/features/rooms/components/EvidenceCards.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { EvidenceCards } from './EvidenceCards';

const mockEvidence = [
  { id: '1', stance: 'pro' as const, claim: 'Good labor practices', source: 'Reuters' },
  { id: '2', stance: 'con' as const, claim: 'Low wages reported', source: null },
];

describe('EvidenceCards', () => {
  it('renders nothing when evidence is empty', () => {
    const { container } = render(<EvidenceCards evidence={[]} />);
    expect(container.firstChild).toBeNull();
  });

  it('shows evidence count and expands on click', async () => {
    render(<EvidenceCards evidence={mockEvidence} />);
    expect(screen.getByText('2 evidence cards')).toBeInTheDocument();

    // Claims should be hidden initially (collapsed)
    expect(screen.queryByText('Good labor practices')).not.toBeVisible();

    // Click to expand
    await userEvent.click(screen.getByText('2 evidence cards'));
    expect(screen.getByText('Good labor practices')).toBeVisible();
    expect(screen.getByText('Low wages reported')).toBeVisible();
    expect(screen.getByText('— Reuters')).toBeVisible();
  });
});
```

**Step 4: Run frontend tests**

Run: `cd web && yarn vitest src/features/rooms/components/EvidenceCards.test.tsx`
Expected: PASS

**Step 5: Commit**

```bash
git add web/src/features/rooms/api/client.ts web/src/features/rooms/components/EvidenceCards.tsx web/src/features/rooms/components/EvidenceCards.test.tsx
git commit -m "feat(web): add Evidence type and EvidenceCards component"
```

---

### Task 5: Frontend integration — wire evidence into Poll page

**Files:**
- Modify: `web/src/pages/Poll.page.tsx` (~L254 — dimension rendering loop)

**Step 1: Import and render EvidenceCards**

In `Poll.page.tsx`, import the `EvidenceCards` component. In the dimension rendering loop (~L254), add `<EvidenceCards evidence={dimension.evidence} />` between the dimension title and the `VoteSlider`:

```tsx
{dimensions.map((dim) => (
  <Card key={dim.id} withBorder>
    <Text fw={500}>{dim.name}</Text>
    {dim.description && <Text size="sm" c="dimmed">{dim.description}</Text>}
    <EvidenceCards evidence={dim.evidence ?? []} />
    <VoteSlider
      dimension={dim}
      value={votes[dim.id] ?? (dim.min_value + dim.max_value) / 2}
      onChange={(v) => setVotes((prev) => ({ ...prev, [dim.id]: v }))}
      disabled={!canVote}
    />
  </Card>
))}
```

Note: Use `dim.evidence ?? []` for backwards compatibility with polls that have no evidence.

**Step 2: Verify visually**

Run: `just dev-frontend`
Navigate to a poll page. Evidence cards section should appear (empty for existing polls). No layout breakage.

**Step 3: Commit**

```bash
git add web/src/pages/Poll.page.tsx
git commit -m "feat(web): wire EvidenceCards into poll page"
```

---

### Task 6: Sim config — add brand ethics config fields

**Files:**
- Modify: `service/src/sim/config.rs` (~L7 — `SimConfig` struct)

**Step 1: Add new fields to SimConfig**

```rust
// Add to SimConfig struct:
pub room_topic: String,        // default "civic", SIM_ROOM_TOPIC
pub company_count: usize,      // default 25, SIM_COMPANY_COUNT
```

`poll_duration_secs` already exists (default 86400).

**Step 2: Add defaults**

In the `Default` impl or figment config, set:
- `room_topic` default: `"civic"`
- `company_count` default: `25`

**Step 3: Verify it compiles**

Run: `cd service && cargo check --bin sim`
Expected: Compiles. Existing behavior unchanged (room_topic defaults to "civic").

**Step 4: Commit**

```bash
git add service/src/sim/config.rs
git commit -m "feat(sim): add room_topic and company_count config fields"
```

---

### Task 7: Sim LLM — brand ethics content types and prompts

**Files:**
- Modify: `service/src/sim/llm.rs` (~L15 — content structs, ~L136 — prompts)

**Step 1: Add brand ethics data types**

Add alongside existing `SimContent`/`SimRoom` types:

```rust
/// Response from Phase 1: company curation LLM call
#[derive(Debug, Deserialize)]
pub struct CompanyCuration {
    pub companies: Vec<CuratedCompany>,
}

#[derive(Debug, Deserialize)]
pub struct CuratedCompany {
    pub ticker: String,
    pub name: String,
    pub relevance_hook: String,
}

/// Response from Phase 2: per-company evidence LLM call
#[derive(Debug, Deserialize)]
pub struct CompanyEvidence {
    pub relevance_hook: String,
    pub dimensions: HashMap<String, DimensionEvidence>,
}

#[derive(Debug, Deserialize)]
pub struct DimensionEvidence {
    pub pro: Vec<String>,
    pub con: Vec<String>,
}
```

**Step 2: Add Phase 1 prompt — company curation**

```rust
const BRAND_CURATION_SYSTEM: &str = r#"You are a research analyst selecting companies for an ethical evaluation platform. Your job is to identify S&P 500 companies that deeply affect people's daily lives but have low brand awareness. Deprioritize household tech and retail names (Apple, Amazon, Google, Walmart) — everyone already has opinions on those. Prioritize companies where users would say "I had no idea they were involved in that.""#;

fn brand_curation_user_prompt(count: usize) -> String {
    format!(
        r#"Select {count} S&P 500 companies ranked by "surprising personal relevance" — how much they touch daily life despite low brand awareness.

Return JSON:
{{
  "companies": [
    {{ "ticker": "SYY", "name": "Sysco Corporation", "relevance_hook": "They supply food to 60% of US school cafeterias and most hospital kitchens." }},
    ...
  ]
}}

Return exactly {count} companies. Order from most to least surprisingly relevant."#
    )
}
```

**Step 3: Add Phase 2 prompt — per-company evidence**

```rust
const BRAND_EVIDENCE_SYSTEM: &str = r#"You are a balanced research analyst providing factual context for ethical evaluation. For each dimension, provide 1-2 claims supporting the positive end and 1-2 claims supporting the negative end. Claims should be one sentence, factual in tone, and specific to the company. Include a source attribution if you can cite a specific report or organization; otherwise omit the source."#;

fn brand_evidence_user_prompt(company_name: &str, ticker: &str) -> String {
    format!(
        r#"Provide ethical evidence for {company_name} ({ticker}) across these 5 dimensions:

1. Labor Practices (Exploitative ↔ Exemplary)
2. Environmental Impact (Destructive ↔ Regenerative)
3. Consumer Trust (Deceptive ↔ Transparent)
4. Community Impact (Extractive ↔ Invested)
5. Corporate Governance (Self-Serving ↔ Accountable)

Also provide a "relevance_hook": 2-3 sentences explaining how this company touches an average person's daily life.

Return JSON:
{{
  "relevance_hook": "...",
  "dimensions": {{
    "Labor Practices": {{ "pro": ["claim1"], "con": ["claim1", "claim2"] }},
    "Environmental Impact": {{ "pro": ["claim1"], "con": ["claim1"] }},
    "Consumer Trust": {{ "pro": ["claim1"], "con": ["claim1"] }},
    "Community Impact": {{ "pro": ["claim1"], "con": ["claim1"] }},
    "Corporate Governance": {{ "pro": ["claim1"], "con": ["claim1"] }}
  }}
}}"#
    )
}
```

**Step 4: Add generation functions**

```rust
/// Phase 1: Ask LLM to curate companies from S&P 500
pub async fn generate_company_curation(
    client: &reqwest::Client,
    config: &SimConfig,
    count: usize,
) -> Result<(CompanyCuration, Usage), anyhow::Error> {
    // Same OpenRouter call pattern as generate_content, but with brand curation prompts
    // If config.mock_llm, return a fixed list of 5 mock companies
}

/// Phase 2: Ask LLM to generate evidence for a single company
pub async fn generate_company_evidence(
    client: &reqwest::Client,
    config: &SimConfig,
    company_name: &str,
    ticker: &str,
) -> Result<(CompanyEvidence, Usage), anyhow::Error> {
    // Same OpenRouter call pattern, brand evidence prompts
    // If config.mock_llm, return mock evidence with 1 pro + 1 con per dimension
}
```

**Step 5: Verify it compiles**

Run: `cd service && cargo check --bin sim`
Expected: Compiles.

**Step 6: Commit**

```bash
git add service/src/sim/llm.rs
git commit -m "feat(sim): add brand ethics LLM types, prompts, and generation functions"
```

---

### Task 8: Sim client — add evidence insertion endpoint

**Files:**
- Modify: `service/src/rooms/http/mod.rs` (add POST endpoint for evidence)
- Modify: `service/src/sim/client.rs` (add `add_evidence` method)

The sim binary creates content via HTTP API calls (not direct DB). We need an endpoint to insert evidence.

**Step 1: Add backend endpoint**

Add to the router (~L153):
```
POST /rooms/{room_id}/polls/{poll_id}/dimensions/{dimension_id}/evidence
```

Handler accepts:
```rust
#[derive(Deserialize)]
struct CreateEvidenceBody {
    evidence: Vec<EvidenceItem>,
}

#[derive(Deserialize)]
struct EvidenceItem {
    stance: String,
    claim: String,
    source: Option<String>,
}
```

Calls `evidence::insert_evidence` from the repo. Returns 201 with count.

**Step 2: Add SimClient method**

In `service/src/sim/client.rs`:

```rust
pub async fn add_evidence(
    &self,
    account: &SimAccount,
    room_id: Uuid,
    poll_id: Uuid,
    dimension_id: Uuid,
    evidence: &[NewEvidenceBody],
) -> Result<()> {
    // POST /rooms/{room_id}/polls/{poll_id}/dimensions/{dimension_id}/evidence
}
```

**Step 3: Add delete evidence endpoint for ring buffer**

```
DELETE /rooms/{room_id}/polls/{poll_id}/evidence
```

Calls `evidence::delete_evidence_for_poll`. Returns 200 with count deleted.

Add corresponding `SimClient::delete_poll_evidence` method.

**Step 4: Verify it compiles**

Run: `cd service && cargo check`
Expected: Compiles.

**Step 5: Commit**

```bash
git add service/src/rooms/http/mod.rs service/src/sim/client.rs
git commit -m "feat(api): add evidence create and delete endpoints for sim"
```

---

### Task 9: Sim binary — brand ethics seeding flow

**Files:**
- Modify: `service/src/bin/sim.rs` (~L144 — after voter setup)
- Create: `service/src/sim/brand.rs` (brand ethics orchestration)
- Modify: `service/src/sim/mod.rs` (add `pub mod brand;`)

**Step 1: Create the brand ethics module**

`service/src/sim/brand.rs` orchestrates the full brand ethics seeding:

```rust
use crate::sim::{client::SimClient, config::SimConfig, identity::SimAccount, llm};

/// Fixed dimensions for all brand ethics polls.
pub const DIMENSIONS: &[(&str, &str, &str)] = &[
    ("Labor Practices", "Exploitative", "Exemplary"),
    ("Environmental Impact", "Destructive", "Regenerative"),
    ("Consumer Trust", "Deceptive", "Transparent"),
    ("Community Impact", "Extractive", "Invested"),
    ("Corporate Governance", "Self-Serving", "Accountable"),
];

pub const ROOM_NAME: &str = "Brand Ethics";

/// Seed the brand ethics room. Idempotent — skips if room already exists with content.
pub async fn seed_brand_ethics(
    http: &reqwest::Client,
    client: &SimClient,
    config: &SimConfig,
    admin: &SimAccount,
    verifier_account_id: Option<Uuid>,
) -> Result<(), anyhow::Error> {
    // 1. Check if room exists (list rooms, find by name)
    // 2. If not: Phase 1 LLM call to curate companies
    // 3. Create room with identity_verified constraint
    // 4. For each company:
    //    a. Create draft poll (question = company name, description = relevance_hook)
    //    b. Create 5 fixed dimensions
    //    c. Phase 2 LLM call for evidence
    //    d. Insert evidence via API
    // 5. First poll auto-activates via lifecycle queue
}
```

**Step 2: Wire into sim.rs main()**

After the voter setup block (~L144), add a branch on `config.room_topic`:

```rust
match config.room_topic.as_str() {
    "brand_ethics" => {
        brand::seed_brand_ethics(&http, &client, &config, &admin, verifier_account_id).await?;
    }
    _ => {
        // existing civic content flow (lines 158-233)
    }
}
```

**Step 3: Test with mock LLM**

Run: `SIM_API_URL=http://localhost:3000 SIM_MOCK_LLM=true SIM_ROOM_TOPIC=brand_ethics SIM_COMPANY_COUNT=3 cargo run --bin sim`
Expected: Creates "Brand Ethics" room with 3 mock company polls, 5 dimensions each, mock evidence cards.

**Step 4: Commit**

```bash
git add service/src/sim/brand.rs service/src/sim/mod.rs service/src/bin/sim.rs
git commit -m "feat(sim): add brand ethics seeding mode with LLM-curated companies"
```

---

### Task 10: Sim — ring buffer reset on capacity fill

**Files:**
- Modify: `service/src/sim/brand.rs` (add refill logic)
- Modify: `service/src/sim/client.rs` (add `reset_poll_status` if needed)

**Step 1: Add ring buffer reset logic**

In `brand.rs`, add a function that runs after initial seeding:

```rust
/// Check if the Brand Ethics room needs refilling (all polls closed, none draft/active).
/// If so, reset all polls to draft, delete old evidence, regenerate evidence, let lifecycle reactivate.
pub async fn refill_if_needed(
    http: &reqwest::Client,
    client: &SimClient,
    config: &SimConfig,
    admin: &SimAccount,
) -> Result<(), anyhow::Error> {
    // 1. Get capacity rooms — if Brand Ethics room is in the list, it needs refill
    // 2. For each poll in the room:
    //    a. Delete old evidence via DELETE endpoint
    //    b. Reset poll status to 'draft' (need an endpoint or direct approach)
    //    c. Phase 2 LLM call for fresh evidence
    //    d. Insert new evidence
    // 3. Lifecycle queue will auto-activate first draft poll
}
```

**Note:** Resetting poll status to `draft` may need a new endpoint (`PATCH /rooms/{room_id}/polls/{poll_id}/status`). Check if `rooms_needing_content` already covers the Brand Ethics room when all polls are closed. If the existing capacity fill flow can be reused, prefer that over a new endpoint.

**Step 2: Wire into the brand_ethics branch in sim.rs**

After `seed_brand_ethics`, call `refill_if_needed`. This handles both first run (no-op, polls already have content) and subsequent runs (ring buffer reset if needed).

**Step 3: Test the ring buffer**

Manually close all polls in the Brand Ethics room, then re-run the sim. Verify polls reset to draft with fresh evidence.

**Step 4: Commit**

```bash
git add service/src/sim/brand.rs service/src/sim/client.rs
git commit -m "feat(sim): add ring buffer reset for brand ethics room"
```

---

### Task 11: Sim — vote seeding for brand ethics

**Files:**
- Modify: `service/src/bin/sim.rs` (ensure vote seeding runs for brand ethics mode too)

**Step 1: Verify vote seeding works**

The existing `cast_simulated_votes` at L238 runs after all content creation. It discovers active polls and casts votes. This should work for brand ethics polls without modification — verify by checking that `cast_simulated_votes` doesn't filter by room type.

**Step 2: Run end-to-end with mock LLM**

```bash
SIM_API_URL=http://localhost:3000 SIM_MOCK_LLM=true SIM_ROOM_TOPIC=brand_ethics SIM_COMPANY_COUNT=3 cargo run --bin sim
```

Verify: Brand Ethics room exists, first company poll is active with votes, evidence cards visible in API response.

**Step 3: Commit (if any changes needed)**

---

### Task 12: Integration test — full brand ethics flow

**Files:**
- Add integration test covering: room creation → poll with evidence → API response → ring buffer reset

**Step 1: Write end-to-end test**

Test that:
1. Creating a poll with evidence via the API works
2. `GET /polls/:id` returns evidence nested in dimensions
3. Deleting evidence for a poll works
4. The frontend type contract matches the API response

**Step 2: Run full test suite**

Run: `just test`
Expected: All tests pass.

**Step 3: Run linting**

Run: `just lint`
Expected: Clean.

**Step 4: Commit**

```bash
git commit -m "test: add integration tests for brand ethics evidence flow"
```

---

### Task 13: Test with real LLM and verify frontend

**Step 1: Run sim with real OpenRouter key**

```bash
export SIM_API_URL=http://localhost:3000
export SIM_OPENROUTER_API_KEY=sk-or-...
export SIM_ROOM_TOPIC=brand_ethics
export SIM_COMPANY_COUNT=5
export SIM_POLL_DURATION_SECS=300  # 5min for fast iteration
cargo run --bin sim
```

**Step 2: Open frontend and verify**

Run: `just dev-frontend`
Navigate to rooms list → Brand Ethics room → active company poll.
Verify:
- Company name as heading
- Relevance hook as description
- 5 dimensions with labeled sliders
- Evidence cards collapsed by default, expandable
- Pro cards with green indicator, con with red
- Voting works, results update

**Step 3: Iterate on LLM prompts if needed**

Adjust system/user prompts based on output quality. This is subjective — the prompts in Task 7 are starting points.

**Step 4: Final commit**

```bash
git commit -m "chore: tune brand ethics LLM prompts after testing"
```

---

## Task Dependency Graph

```
Task 1 (migration)
  └─→ Task 2 (repo)
       └─→ Task 3 (API response)
            └─→ Task 5 (frontend integration)
       └─→ Task 8 (evidence endpoints)
            └─→ Task 9 (sim seeding)
                 └─→ Task 10 (ring buffer)
                 └─→ Task 11 (vote seeding)
Task 4 (frontend component) ── independent, can parallel with Tasks 2-3
Task 6 (sim config) ── independent, can parallel with Tasks 2-3
Task 7 (LLM prompts) ── depends on Task 6
Task 12 (integration test) ── after Tasks 1-11
Task 13 (real LLM test) ── after Task 12
```

**Parallelizable pairs:** Tasks 4+6 can run alongside Tasks 2-3. Tasks 7+8 can start once 6 is done.
