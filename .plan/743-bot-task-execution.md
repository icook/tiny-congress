# Bot Task Execution — Stub Filling Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fill the bot worker's task execution stubs with real LLM + Exa logic, extracting shared primitives from the sim to avoid duplication. Add cache header detection for all three cache layers.

**Architecture:** Extract generic LLM/Exa client code from `service/src/sim/llm.rs` into a new `crates/tc-llm/` crate. This is engine-agnostic — any engine, the sim, or future tools can depend on it. Bot worker and sim both import from `tc-llm`. Cache status flows into trace steps.

**Tech Stack:** Rust (reqwest, serde, tokio), OpenRouter-compatible API, Exa search API

---

## Task 1: Extract shared LLM/Exa client module

**Files:**
- Create: `crates/tc-llm/Cargo.toml`
- Create: `crates/tc-llm/src/lib.rs` — types + client functions
- Modify: `Cargo.toml` (workspace) — add `tc-llm` to `members`
- Modify: `crates/tc-engine-polling/Cargo.toml` — add `tc-llm` dependency
- Modify: `service/Cargo.toml` — add `tc-llm` dependency

### Types to extract (from service/src/sim/llm.rs):

```rust
// Re-export existing Usage from wherever it lives, or move it here
pub use crate::repo::bot_traces::TraceStep; // for cache info

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Cache detection results from HTTP response headers
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct CacheInfo {
    /// OpenRouter prompt cache: tokens read from cache
    pub openrouter_cached_tokens: Option<u32>,
    /// LiteLLM proxy: full response served from cache
    pub litellm_hit: bool,
    /// Nginx: response served from cache (for Exa)
    pub nginx_hit: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletion {
    pub content: String,
    pub usage: Usage,
    pub cache: CacheInfo,
    pub generation_id: Option<String>,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub url: String,
    pub title: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
    pub cache: CacheInfo,
}
```

### Functions to extract:

```rust
/// Generic chat completion against any OpenRouter-compatible API.
/// Inspects response headers for cache detection.
pub async fn chat_completion(
    client: &reqwest::Client,
    api_key: &str,
    base_url: &str,
    model: &str,
    messages: &[ChatMessage],
    json_mode: bool,
) -> Result<ChatCompletion, anyhow::Error>
```

Implementation: adapt from the pattern repeated 4x in `service/src/sim/llm.rs`. Key changes:
- Read `x-cache` / `x-litellm-cache-key` headers from the raw `reqwest::Response` BEFORE calling `.json()`
- Extract `usage.prompt_tokens_details.cached_tokens` from response body
- Return `CacheInfo` alongside content

```rust
/// Search via Exa API (or nginx cache proxy).
/// Inspects X-Cache header for nginx cache detection.
pub async fn exa_search(
    client: &reqwest::Client,
    api_key: &str,
    base_url: &str,
    query: &str,
    num_results: usize,
) -> Result<SearchResponse, anyhow::Error>
```

Implementation: adapt from private `exa_search` in `service/src/sim/llm.rs`. Key changes:
- Read `X-Cache` header from response for nginx detection
- Return `SearchResponse` with `CacheInfo`

```rust
/// Strip markdown code fences from LLM output.
pub fn extract_json(raw: &str) -> &str
```

Copy from `service/src/sim/llm.rs` — pure function, no changes needed.

```rust
/// Fetch realized cost from OpenRouter's generation endpoint.
pub async fn get_generation_cost(
    client: &reqwest::Client,
    api_key: &str,
    base_url: &str,
    generation_id: &str,
) -> Result<Option<f64>, anyhow::Error>
```

Copy from `service/src/sim/llm.rs` — already takes raw strings.

### Cache header detection specifics:

**LiteLLM responses** — check for:
- `x-litellm-cache-hit: True` header (LiteLLM's cache indicator)
- If absent, check `x-cache: HIT` as fallback

**Nginx/Exa responses** — check for:
- `X-Cache: HIT` or `X-Cache-Status: HIT` headers (standard nginx cache headers)

**OpenRouter prompt cache** — from response body:
- `usage.prompt_tokens_details.cached_tokens > 0`

### Step-by-step:

1. Add `reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }` to `crates/tc-engine-polling/Cargo.toml`
2. Create `llm.rs` with the types above
3. Implement `chat_completion` — build request, send, read headers for cache, parse body, return ChatCompletion
4. Implement `exa_search` — build request, send, read X-Cache header, parse body, return SearchResponse
5. Copy `extract_json` and `get_generation_cost` from sim
6. Add `tc-llm` to workspace members in root `Cargo.toml`
7. Add `tc-llm` as dependency in `tc-engine-polling/Cargo.toml` and `service/Cargo.toml`
8. `cargo check` (full workspace)
9. Commit: `feat(743): extract shared LLM/Exa client crate with cache detection`

---

## Task 2: Refactor sim to use shared primitives

**Files:**
- Modify: `service/src/sim/llm.rs` — replace inlined HTTP calls with `tc_engine_polling::tc_llm::*`

### What changes:

The 4 places in `llm.rs` that build+send a chat completion request should call `tc_llm::chat_completion()` instead. The sim's higher-level functions (`generate_content`, `generate_company_curation`, `generate_company_evidence`, etc.) keep their signatures — they just internally delegate to the shared primitive.

Pattern:
```rust
// Before (inlined in each generate_* function):
let response = client.post(url).header("Authorization", ...).json(&request).send().await?;
let chat_response: ChatResponse = response.json().await?;

// After:
let completion = tc_engine_polling::tc_llm::chat_completion(
    client, &config.openrouter_api_key, &config.llm_base_url,
    &config.openrouter_model, &messages, true,
).await?;
```

Similarly for `exa_search` — the private function becomes a call to the shared one.

`extract_json`, `get_generation_cost`, `Usage`, `ChatMessage` — import from shared module, remove local definitions.

### Step-by-step:

1. Replace `Usage` struct with re-export from shared module
2. Replace `ChatMessage` with re-export
3. Replace `extract_json` with re-export
4. Replace `get_generation_cost` with re-export
5. Replace each inlined chat completion HTTP call with `tc_llm::chat_completion()`
6. Replace private `exa_search` with call to shared `tc_llm::exa_search()`
7. Remove now-dead private types (`ChatRequest`, `ChatResponse`, `ResponseFormat`, etc.)
8. `cargo test -p tc-service` — all 67 sim tests must still pass
9. `cargo check` — full workspace
10. Commit: `refactor(743): sim uses shared LLM/Exa client from tc-engine-polling`

---

## Task 3: Implement bot task execution

**Files:**
- Modify: `crates/tc-engine-polling/src/bot/worker.rs` — fill `dispatch_task` stubs
- Create: `crates/tc-engine-polling/src/bot/tasks.rs` — task execution logic
- Modify: `crates/tc-engine-polling/src/bot/mod.rs` — add `pub mod tasks;`

### Task types and their execution:

**`research_company`** — the main task:
1. Parse params: `{ "company": "Apple Inc.", "ticker": "AAPL" }`
2. Read room's `engine_config` for bot config (model, quality, search_provider)
3. Build research prompt (what dimensions to investigate)
4. Call `exa_search()` for each dimension → trace steps
5. Call `chat_completion()` to synthesize evidence from search results → trace step
6. Parse output into evidence items (pro/con per dimension)
7. Create poll + dimensions + evidence in DB
8. Complete trace with poll_id

**`generate_evidence`** — re-generate evidence for existing poll:
1. Parse params: `{ "poll_id": "..." }`
2. Load poll dimensions from DB
3. Call `exa_search()` + `chat_completion()` per dimension
4. Insert evidence items
5. Complete trace

### Prompt templates:

For now, hardcode the prompts in `tasks.rs`. They should match what the sim currently uses in `build_brand_evidence_messages` and `build_exa_synthesis_messages`. These will become configurable in #771.

### Worker changes:

Update `dispatch_task` in `worker.rs` to:
1. Load the room from DB (need `engine_config` for bot settings)
2. Create a `reqwest::Client` (or receive one from the worker)
3. Delegate to `tasks::research_company()` or `tasks::generate_evidence()`
4. Each task function receives `&PgPool`, `&reqwest::Client`, `&BotTask`, and returns `Result<Option<Uuid>, anyhow::Error>` (the poll_id if one was created)

The worker should hold a `reqwest::Client` and pass it to tasks — don't create a new client per task.

### reqwest::Client in worker:

Update `spawn_bot_worker` signature to also take config for API keys and URLs:

```rust
pub struct BotWorkerConfig {
    pub llm_api_key: String,
    pub llm_base_url: String,
    pub exa_api_key: String,
    pub exa_base_url: String,
}

pub fn spawn_bot_worker(pool: PgPool, config: BotWorkerConfig) -> JoinHandle<()>
```

The config comes from environment variables, loaded in `engine.rs` `start()`. Follow the sim's `SIM_*` pattern but use `BOT_*` prefix, or read from the engine context.

### Step-by-step:

1. Create `tasks.rs` with `research_company()` and `generate_evidence()` functions
2. Add prompt templates for research + evidence synthesis
3. Update `worker.rs` — add `BotWorkerConfig`, reqwest::Client, delegate to tasks
4. Update `engine.rs` — load config, pass to `spawn_bot_worker`
5. Each task function: search → LLM → parse → DB insert → trace steps
6. `cargo check -p tc-engine-polling`
7. `cargo check` (full workspace)
8. Commit: `feat(743): implement research_company and generate_evidence bot tasks`

---

## Task 4: Port-forward recipe + manual test

**Files:**
- Modify: `justfile` — add `pf-bot` and `bot-run` recipes

### Recipes:

```just
# Port-forward LiteLLM and Exa cache from cluster
pf-bot:
    kubectl port-forward -n tiny-congress-demo svc/litellm 4001:4001 &
    kubectl port-forward -n tiny-congress-demo svc/exa-cache 4002:4002 &
    echo "LiteLLM: localhost:4001, Exa cache: localhost:4002"
    wait

# Run a single bot task against local port-forwards
bot-run company="Apple Inc." room_id="a1111111-1111-1111-1111-111111111111":
    @echo "Enqueuing research_company task..."
    kubectl exec -n tiny-congress-demo deploy/tc-demo-postgres -- psql -U postgres -d tiny_congress \
        -c "SELECT pgmq.send('rooms__bot_tasks', '{\"room_id\": \"{{room_id}}\", \"task\": \"research_company\", \"params\": {\"company\": \"{{company}}\"}}'::jsonb);"
    @echo "Watching traces..."
    kubectl exec -n tiny-congress-demo deploy/tc-demo-postgres -- psql -U postgres -d tiny_congress \
        -c "SELECT id, task, status, total_cost_usd, steps->0->>'output_summary' as first_step FROM rooms__bot_traces WHERE room_id = '{{room_id}}' ORDER BY created_at DESC LIMIT 5;"
```

### Step-by-step:

1. Add recipes to justfile
2. Test `just pf-bot` — verify services are reachable
3. Test `just bot-run` — verify task is enqueued and processed
4. Commit: `feat(743): add bot port-forward and manual run recipes`

---

## Dependency Graph

```
Task 1 (extract shared client) ──┬── Task 2 (refactor sim)
                                  └── Task 3 (implement bot tasks) ── Task 4 (just recipes)
```

Tasks 2 and 3 can run in parallel after Task 1. Task 4 depends on 3 (needs working task execution to test).

## Not in scope

- Operational CLI binary (`tc-ops`) — separate ticket
- Configurable prompts (#771)
- Bot scheduling (enqueue on timer) — the manual `just bot-run` recipe is sufficient for now
- Frontend trace viewer already exists from earlier tasks
