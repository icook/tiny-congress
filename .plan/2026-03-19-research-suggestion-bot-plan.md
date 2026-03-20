# Research Suggestion Bot Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the `research_suggestion` stub with a real LLM+Exa pipeline that turns user suggestions into evidence on existing poll dimensions.

**Architecture:** Suggestions become poll-scoped (new `poll_id` column + route change). The `research_suggestion` bot task does: LLM reformulates suggestion into 2-3 search queries → parallel Exa searches → LLM synthesizes into 2-4 pro/con evidence claims → inserts evidence on best-fit dimension → updates suggestion with evidence_ids. Single-task pipeline following the existing `research_company` pattern.

**Tech Stack:** Rust/axum (API), sqlx/Postgres (data), pgmq (task queue), tc_llm crate (LLM + Exa), TanStack Query + Mantine (frontend)

---

### Task 1: Migration — add poll_id to suggestions table

**Files:**
- Create: `service/migrations/26_suggestions_poll_scope.sql`

**Step 1: Write the migration**

```sql
-- Add poll_id to scope suggestions to a specific poll
ALTER TABLE rooms__research_suggestions
  ADD COLUMN poll_id UUID REFERENCES rooms__poll_polls(id);

-- Backfill: no existing rows expected in prod, but handle gracefully
-- For any existing rows, we can't infer poll_id so leave NULL

-- Now make it NOT NULL for new rows via a check constraint
-- (can't use NOT NULL directly without backfill)
-- Actually: this is pre-launch demo, no prod data. Just add NOT NULL.
ALTER TABLE rooms__research_suggestions
  ALTER COLUMN poll_id SET NOT NULL;

-- Index for listing suggestions by poll
CREATE INDEX IF NOT EXISTS idx_suggestions_poll_status
  ON rooms__research_suggestions(poll_id, status, created_at);
```

**Step 2: Verify migration numbering**

Run: `ls service/migrations/*.sql | sort -V | tail -3`
Expected: migration 25 is the last one. If not, renumber to next available.

**Step 3: Commit**

```bash
git add service/migrations/26_suggestions_poll_scope.sql
git commit -m "feat(migration): add poll_id to research suggestions table (#852)"
```

---

### Task 2: Backend — update repo layer for poll-scoped suggestions

**Files:**
- Modify: `service/src/rooms/repo/suggestions.rs`

The repo functions need `poll_id` threaded through.

**Step 1: Update `SuggestionRecord` struct**

In `service/src/rooms/repo/suggestions.rs`, add `poll_id: Uuid` field to the `SuggestionRecord` struct (after `room_id`). Also add it to `SuggestionRow` if there's a separate row type.

**Step 2: Update `create_suggestion` function**

Add `poll_id: Uuid` parameter. Update the INSERT to include `poll_id`:

```rust
pub async fn create_suggestion<'e, E: sqlx::Executor<'e, Database = sqlx::Postgres>>(
    executor: E,
    room_id: Uuid,
    poll_id: Uuid,
    account_id: Uuid,
    suggestion_text: &str,
    status: &str,
    filter_reason: Option<&str>,
) -> Result<SuggestionRecord, SuggestionRepoError>
```

INSERT SQL becomes:
```sql
INSERT INTO rooms__research_suggestions (room_id, poll_id, account_id, suggestion_text, status, filter_reason)
VALUES ($1, $2, $3, $4, $5, $6)
RETURNING *
```

**Step 3: Update `list_suggestions` function**

Change signature to accept `poll_id: Uuid` instead of (or in addition to) `room_id`. Filter by `poll_id`:

```rust
pub async fn list_suggestions<'e, E: sqlx::Executor<'e, Database = sqlx::Postgres>>(
    executor: E,
    poll_id: Uuid,
) -> Result<Vec<SuggestionRecord>, SuggestionRepoError>
```

WHERE clause: `WHERE poll_id = $1 ORDER BY created_at DESC`

**Step 4: Update `claim_next_queued`**

The claim function currently scopes by `room_id`. For `process_suggestions`, we still want room-level claiming (process all queued suggestions in the room). Keep the existing signature but ensure the returned record includes `poll_id`.

**Step 5: Run backend tests**

Run: `cd /Users/icook/tiny-congress-4/.claude/worktrees/feature/757-steerable-research && cargo test --test rooms_handler_tests 2>&1 | tail -20`
Expected: Compilation errors (handler tests not updated yet — that's expected at this step)

**Step 6: Commit**

```bash
git add service/src/rooms/repo/suggestions.rs
git commit -m "feat(repo): add poll_id to suggestion CRUD functions (#852)"
```

---

### Task 3: Backend — update HTTP handlers and routes for poll-scoped suggestions

**Files:**
- Modify: `service/src/rooms/http/mod.rs` (routes + types)
- Modify: `service/src/rooms/http/platform.rs` (handlers)

**Step 1: Update `SuggestionResponse` to include `poll_id`**

In `service/src/rooms/http/mod.rs` (~line 106), add `poll_id: uuid::Uuid` to `SuggestionResponse`.

**Step 2: Update route from room-scoped to poll-scoped**

In `service/src/rooms/http/mod.rs`, change the suggestions route (~line 134-138):

Old:
```rust
.route("/rooms/{room_id}/suggestions",
    get(platform::list_suggestions).post(platform::create_suggestion))
```

New:
```rust
.route("/rooms/{room_id}/polls/{poll_id}/suggestions",
    get(platform::list_suggestions).post(platform::create_suggestion))
```

**Step 3: Update `create_suggestion` handler**

In `service/src/rooms/http/platform.rs` (~line 307), add `Path` extraction for both `room_id` and `poll_id`:

```rust
pub async fn create_suggestion(
    Extension(pool): Extension<PgPool>,
    Extension(content_filter): Extension<Arc<dyn ContentFilter>>,
    Path((room_id, poll_id)): Path<(Uuid, Uuid)>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse
```

Pass `poll_id` through to `suggestions::create_suggestion(...)`.

**Step 4: Update `list_suggestions` handler**

```rust
pub async fn list_suggestions(
    Extension(pool): Extension<PgPool>,
    Path((_room_id, poll_id)): Path<(Uuid, Uuid)>,
) -> impl IntoResponse
```

Call `suggestions::list_suggestions(&pool, poll_id)`.

**Step 5: Update `suggestion_to_response` converter**

Add `poll_id: s.poll_id` to the response mapping (~line 380).

**Step 6: Run backend tests**

Run: `cargo test --test rooms_handler_tests 2>&1 | tail -30`
Expected: Tests fail because test helpers don't pass `poll_id` yet.

**Step 7: Commit**

```bash
git add service/src/rooms/http/mod.rs service/src/rooms/http/platform.rs
git commit -m "feat(http): poll-scoped suggestion routes (#852)"
```

---

### Task 4: Backend — update handler tests

**Files:**
- Modify: `service/tests/rooms_handler_tests.rs`

**Step 1: Update existing suggestion tests**

The 5 existing suggestion tests (`test_create_suggestion`, `test_create_suggestion_empty_text`, `test_create_suggestion_too_long`, `test_list_suggestions`, `test_suggestion_rate_limit`) currently POST to `/rooms/{room_id}/suggestions`.

Each test needs:
1. Create a poll first (POST to `/rooms/{room_id}/polls` with a valid poll body)
2. Change the suggestion URL to `/rooms/{room_id}/polls/{poll_id}/suggestions`

Follow the pattern from existing poll tests in the same file for creating a poll.

**Step 2: Run tests**

Run: `cargo test --test rooms_handler_tests 2>&1 | tail -30`
Expected: All 5 suggestion tests pass (plus existing tests).

**Step 3: Update schema snapshot if needed**

Run: `INSTA_UPDATE=always cargo test --test db_tests test_schema_matches_snapshot 2>&1 | tail -10`

**Step 4: Commit**

```bash
git add service/tests/rooms_handler_tests.rs
git add -u service/tests/snapshots/  # if snapshot changed
git commit -m "test: update suggestion tests for poll-scoped routes (#852)"
```

---

### Task 5: Bot — implement research_suggestion pipeline

This is the core task. Replace the stub in `crates/tc-engine-polling/src/bot/tasks.rs`.

**Files:**
- Modify: `crates/tc-engine-polling/src/bot/tasks.rs` (lines 598-627)

**Step 1: Add the LLM prompt constants**

Near the top of `tasks.rs` (after the existing `EXA_SYNTHESIS_SYSTEM` constant), add:

```rust
const SUGGESTION_QUERY_SYSTEM: &str = "\
You are a research assistant. Given a user's suggestion and the context of a poll, \
generate 2-3 focused web search queries that would find relevant evidence. \
Return JSON: {\"queries\": [\"query1\", \"query2\", ...]}";

const SUGGESTION_SYNTHESIS_SYSTEM: &str = "\
You are a research analyst. Given search results about a topic, extract 2-4 evidence claims. \
Each claim should be a clear factual assertion with a pro or con stance. \
Return JSON: {\"dimension_name\": \"<best fit dimension or new name>\", \
\"evidence\": [{\"stance\": \"pro\"|\"con\", \"claim\": \"...\", \"source\": \"url\"}]}";
```

**Step 2: Add response structs**

```rust
#[derive(Debug, Deserialize)]
struct SuggestionQueries {
    queries: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct SuggestionEvidence {
    dimension_name: String,
    evidence: Vec<SuggestionEvidenceItem>,
}

#[derive(Debug, Deserialize)]
struct SuggestionEvidenceItem {
    stance: String,
    claim: String,
    source: Option<String>,
}
```

**Step 3: Implement `research_suggestion`**

Replace the stub (lines 598-627) with the full implementation. Follow the `research_company` pattern:

```rust
pub async fn research_suggestion(
    pool: &PgPool,
    http: &reqwest::Client,
    config: &BotWorkerConfig,
    task: &BotTask,
    trace_id: Uuid,
) -> anyhow::Result<Option<Uuid>> {
    let suggestion_id: Uuid = task.params["suggestion_id"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .context("missing suggestion_id")?;
    let suggestion_text = task.params["suggestion_text"]
        .as_str()
        .context("missing suggestion_text")?;
    let poll_id: Uuid = task.params["poll_id"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .context("missing poll_id")?;
    let room_id: Uuid = task.params["room_id"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .context("missing room_id")?;

    // 1. Load poll context (topic + existing dimensions)
    let poll = super::super::repo::polls::get_poll(pool, poll_id)
        .await?
        .context("poll not found")?;
    let dimensions = super::super::repo::polls::list_dimensions(pool, poll_id).await?;
    let dim_names: Vec<&str> = dimensions.iter().map(|d| d.name.as_str()).collect();

    // Check for per-room model override
    let room_engine_config: serde_json::Value = sqlx::query_scalar(
        "SELECT engine_config FROM rooms__rooms WHERE id = $1"
    )
    .bind(room_id)
    .fetch_one(pool)
    .await
    .unwrap_or_default();
    let model = room_engine_config
        .pointer("/bot/model")
        .and_then(|v| v.as_str())
        .unwrap_or(&config.default_model);

    // 2. LLM: generate search queries
    let query_prompt = format!(
        "Poll topic: {}\nExisting dimensions: {}\nUser suggestion: {}\n\n\
         Generate 2-3 search queries to find evidence related to this suggestion.",
        poll.question,
        dim_names.join(", "),
        suggestion_text
    );
    let query_messages = vec![
        tc_llm::Message { role: "system".into(), content: SUGGESTION_QUERY_SYSTEM.into() },
        tc_llm::Message { role: "user".into(), content: query_prompt },
    ];

    let t0 = std::time::Instant::now();
    let query_completion = tc_llm::chat_completion(
        http, &config.llm_api_key, &config.llm_base_url,
        model, &query_messages, true, Some(0.3),
    ).await?;
    let query_ms = t0.elapsed().as_millis() as i64;

    // Emit trace step
    let _ = bot_traces::append_step(pool, trace_id, &bot_traces::TraceStep {
        step_type: "query_generation".into(),
        model: Some(model.into()),
        query: Some(suggestion_text.into()),
        input_tokens: query_completion.usage.as_ref().map(|u| u.prompt_tokens as i64),
        output_tokens: query_completion.usage.as_ref().map(|u| u.completion_tokens as i64),
        latency_ms: Some(query_ms),
        cost_usd: None,
        cache: query_completion.cache.clone(),
        output_summary: Some(query_completion.content.clone()),
    }).await;

    let queries: SuggestionQueries = serde_json::from_str(
        &tc_llm::extract_json(&query_completion.content)
    ).context("failed to parse query generation response")?;

    // 3. Parallel Exa searches
    let mut search_results = Vec::new();
    let search_futures: Vec<_> = queries.queries.iter().take(3).map(|q| {
        tc_llm::exa_search(http, &config.exa_api_key, &config.exa_base_url, q, 5)
    }).collect();

    let results = futures::future::join_all(search_futures).await;
    for (i, result) in results.into_iter().enumerate() {
        let query = queries.queries.get(i).map(|s| s.as_str()).unwrap_or("");
        match result {
            Ok(resp) => {
                let _ = bot_traces::append_step(pool, trace_id, &bot_traces::TraceStep {
                    step_type: "exa_search".into(),
                    model: None,
                    query: Some(query.into()),
                    input_tokens: None,
                    output_tokens: None,
                    latency_ms: None,
                    cost_usd: None,
                    cache: resp.cache.clone(),
                    output_summary: Some(format!("{} results", resp.results.len())),
                }).await;
                search_results.extend(resp.results);
            }
            Err(e) => {
                tracing::warn!(query, error = %e, "exa search failed, continuing");
            }
        }
    }

    if search_results.is_empty() {
        // No search results — mark as failed
        let _ = sqlx::query(
            "UPDATE rooms__research_suggestions SET status = 'failed', \
             filter_reason = 'No search results found', processed_at = now() WHERE id = $1"
        ).bind(suggestion_id).execute(pool).await;
        anyhow::bail!("no search results for suggestion {suggestion_id}");
    }

    // 4. Build search context string
    let search_context: String = search_results.iter().take(15).map(|r| {
        format!("Title: {}\nURL: {}\nSnippet: {}\n---",
            r.title.as_deref().unwrap_or(""),
            r.url,
            r.text.as_deref().unwrap_or("").chars().take(500).collect::<String>())
    }).collect::<Vec<_>>().join("\n");

    // 5. LLM: synthesize evidence
    let synthesis_prompt = format!(
        "Poll topic: {}\nExisting dimensions: {}\nUser suggestion: {}\n\n\
         Search results:\n{}\n\n\
         Extract 2-4 evidence claims. Pick the best-fit existing dimension name, \
         or suggest a new one if none fit. Each claim needs a stance (pro/con), \
         the claim text, and a source URL.",
        poll.question,
        dim_names.join(", "),
        suggestion_text,
        search_context
    );
    let synth_messages = vec![
        tc_llm::Message { role: "system".into(), content: SUGGESTION_SYNTHESIS_SYSTEM.into() },
        tc_llm::Message { role: "user".into(), content: synthesis_prompt },
    ];

    let t0 = std::time::Instant::now();
    let synth_completion = tc_llm::chat_completion(
        http, &config.llm_api_key, &config.llm_base_url,
        model, &synth_messages, true, Some(0.3),
    ).await?;
    let synth_ms = t0.elapsed().as_millis() as i64;

    let _ = bot_traces::append_step(pool, trace_id, &bot_traces::TraceStep {
        step_type: "llm_synthesis".into(),
        model: Some(model.into()),
        query: None,
        input_tokens: synth_completion.usage.as_ref().map(|u| u.prompt_tokens as i64),
        output_tokens: synth_completion.usage.as_ref().map(|u| u.completion_tokens as i64),
        latency_ms: Some(synth_ms),
        cost_usd: None,
        cache: synth_completion.cache.clone(),
        output_summary: Some(synth_completion.content.chars().take(500).collect()),
    }).await;

    let evidence: SuggestionEvidence = serde_json::from_str(
        &tc_llm::extract_json(&synth_completion.content)
    ).context("failed to parse synthesis response")?;

    // 6. Find or create the target dimension
    let target_dim = dimensions.iter().find(|d| {
        d.name.to_lowercase() == evidence.dimension_name.to_lowercase()
    });

    let dimension_id = if let Some(dim) = target_dim {
        dim.id
    } else {
        // Create new dimension
        let sort_order = dimensions.len() as i32;
        let new_dim = super::super::repo::polls::create_dimension(
            pool, poll_id, &evidence.dimension_name, None,
            0.0, 100.0, sort_order, None, None,
        ).await?;
        new_dim.id
    };

    // 7. Insert evidence
    let new_evidence: Vec<NewEvidence> = evidence.evidence.iter().map(|e| {
        NewEvidence {
            stance: &e.stance,
            claim: &e.claim,
            source: e.source.as_deref(),
        }
    }).collect();

    evidence_repo::insert_evidence(pool, dimension_id, &new_evidence).await?;

    // Get the inserted evidence IDs (fetch most recent for this dimension)
    let evidence_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM rooms__poll_evidence WHERE dimension_id = $1 \
         ORDER BY created_at DESC LIMIT $2"
    )
    .bind(dimension_id)
    .bind(new_evidence.len() as i64)
    .fetch_all(pool)
    .await?;

    // 8. Update suggestion as completed
    sqlx::query(
        "UPDATE rooms__research_suggestions \
         SET status = 'completed', evidence_ids = $2, processed_at = now() \
         WHERE id = $1"
    )
    .bind(suggestion_id)
    .bind(&evidence_ids)
    .execute(pool)
    .await?;

    Ok(Some(poll_id))
}
```

**Step 4: Update `process_suggestions` to pass `poll_id` in params**

In the `process_suggestions` function (~line 537), the pgmq task enqueue currently sends `suggestion_id` and `suggestion_text`. Add `poll_id` and `room_id`:

```rust
serde_json::json!({
    "suggestion_id": suggestion.id.to_string(),
    "suggestion_text": suggestion.suggestion_text,
    "poll_id": suggestion.poll_id.to_string(),
    "room_id": suggestion.room_id.to_string(),
})
```

**Step 5: Add missing imports at top of tasks.rs**

Ensure these are imported:
```rust
use super::super::repo::evidence as evidence_repo;
use super::super::repo::evidence::NewEvidence;
```

Check existing imports — `evidence_repo` may already be imported for `generate_evidence`. If not, add it.

**Step 6: Verify it compiles**

Run: `cargo check -p tc-engine-polling 2>&1 | tail -20`
Expected: Compiles successfully (may need to adjust exact import paths or struct field access)

**Step 7: Commit**

```bash
git add crates/tc-engine-polling/src/bot/tasks.rs
git commit -m "feat(bot): implement research_suggestion LLM+Exa pipeline (#852)"
```

---

### Task 6: Frontend — update API client and hooks for poll-scoped suggestions

**Files:**
- Modify: `web/src/engines/polling/api/client.ts`
- Modify: `web/src/engines/polling/api/queries.ts`

**Step 1: Update `Suggestion` interface**

In `client.ts` (~line 111), add `poll_id: string` to the `Suggestion` interface.

**Step 2: Update `listSuggestions` function**

Change signature and URL:
```typescript
export async function listSuggestions(roomId: string, pollId: string): Promise<Suggestion[]> {
  return fetchJson(`/rooms/${roomId}/polls/${pollId}/suggestions`);
}
```

**Step 3: Update `createSuggestion` function**

Add `pollId` parameter, update URL:
```typescript
export async function createSuggestion(
  roomId: string,
  pollId: string,
  suggestionText: string,
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule,
): Promise<Suggestion> {
  return signedFetchJson(
    `/rooms/${roomId}/polls/${pollId}/suggestions`,
    // ... rest stays the same
  );
}
```

**Step 4: Update `useSuggestions` hook**

In `queries.ts` (~line 161):
```typescript
export function useSuggestions(roomId: string, pollId: string) {
  return useQuery<Suggestion[]>({
    queryKey: ['suggestions', roomId, pollId],
    queryFn: () => listSuggestions(roomId, pollId),
    enabled: Boolean(roomId && pollId),
    refetchInterval: 15_000,
  });
}
```

**Step 5: Update `useCreateSuggestion` hook**

In `queries.ts` (~line 170):
```typescript
export function useCreateSuggestion(
  roomId: string,
  pollId: string,
  deviceKid: string | null,
  privateKey: CryptoKey | null,
  wasmCrypto: CryptoModule | null
) {
  const queryClient = useQueryClient();
  return useMutation<Suggestion, Error, string>({
    mutationFn: async (suggestionText: string) => {
      if (!deviceKid || !privateKey || !wasmCrypto) throw new Error('Not authenticated');
      return createSuggestion(roomId, pollId, suggestionText, deviceKid, privateKey, wasmCrypto);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['suggestions', roomId, pollId] });
    },
  });
}
```

**Step 6: Update barrel export if needed**

Check `web/src/engines/polling/api/index.ts` — make sure re-exports include the updated signatures.

**Step 7: Run type check**

Run: `cd web && yarn tsc --noEmit 2>&1 | tail -20`
Expected: Type errors in SuggestionFeed.tsx (not updated yet — expected)

**Step 8: Commit**

```bash
git add web/src/engines/polling/api/client.ts web/src/engines/polling/api/queries.ts
git commit -m "feat(api): poll-scoped suggestion client and hooks (#852)"
```

---

### Task 7: Frontend — move SuggestionFeed into poll detail view

**Files:**
- Modify: `web/src/engines/polling/components/SuggestionFeed.tsx`
- Modify: `web/src/engines/polling/PollEngineView.tsx`
- Find and modify: the poll detail page/component (search for where `usePollDetail` is called)

**Step 1: Update SuggestionFeed props**

Change from `roomId` to `roomId` + `pollId`:
```typescript
interface SuggestionFeedProps {
  roomId: string;
  pollId: string;
}

export function SuggestionFeed({ roomId, pollId }: SuggestionFeedProps) {
  // Update hook calls
  const { data: suggestions = [], isLoading } = useSuggestions(roomId, pollId);
  // ...
  const createSuggestion = useCreateSuggestion(roomId, pollId, deviceKid, privateKey, crypto);
```

**Step 2: Remove SuggestionFeed from PollEngineView**

In `PollEngineView.tsx` (~line 66), remove the `<SuggestionFeed roomId={roomId} />` line and its import.

**Step 3: Add SuggestionFeed to poll detail page**

Find the component that renders a single poll's detail view (where `usePollDetail` is called). Add `<SuggestionFeed roomId={roomId} pollId={pollId} />` below the evidence/dimensions section.

This will require exploring the route structure — look at `web/src/engines/polling/` for a `PollDetailView` or similar component, or check `routeTree.gen.ts` for poll detail routes.

**Step 4: Run type check and lint**

Run: `cd web && yarn tsc --noEmit && yarn eslint src/engines/polling/ 2>&1 | tail -20`
Expected: Clean

**Step 5: Commit**

```bash
git add web/src/engines/polling/
git commit -m "feat(ui): move SuggestionFeed into poll detail view (#852)"
```

---

### Task 8: Frontend — update SuggestionFeed tests

**Files:**
- Modify: `web/src/engines/polling/components/SuggestionFeed.test.tsx`

**Step 1: Update test mocks and renders**

All 4 tests pass `roomId="room-1"` — update to also pass `pollId="poll-1"`:

```typescript
render(<SuggestionFeed roomId="room-1" pollId="poll-1" />);
```

Update mock assertions: `mockUseSuggestions` is called with `(roomId, pollId)` and `mockUseCreateSuggestion` with `(roomId, pollId, ...)`.

**Step 2: Update Suggestion mock data**

Add `poll_id: 'poll-1'` to each mock suggestion object in the test data.

**Step 3: Run tests**

Run: `cd web && yarn vitest src/engines/polling/components/SuggestionFeed.test.tsx 2>&1 | tail -20`
Expected: All 4 tests pass

**Step 4: Commit**

```bash
git add web/src/engines/polling/components/SuggestionFeed.test.tsx
git commit -m "test: update SuggestionFeed tests for poll-scoped props (#852)"
```

---

### Task 9: Full verification

**Step 1: Run all backend tests**

Run: `cargo test 2>&1 | tail -30`
Expected: All tests pass

**Step 2: Run all frontend tests**

Run: `cd web && yarn vitest --run 2>&1 | tail -30`
Expected: All tests pass

**Step 3: Run linting**

Run: `just lint 2>&1 | tail -30`
Expected: Clean

**Step 4: Update schema snapshot if needed**

Run: `INSTA_UPDATE=always cargo test --test db_tests test_schema_matches_snapshot 2>&1 | tail -10`

**Step 5: Final commit if snapshot changed**

```bash
git add -u service/tests/snapshots/
git commit -m "fix: update schema snapshot for migration 26"
```
