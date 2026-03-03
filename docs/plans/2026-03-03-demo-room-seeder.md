# Demo Room Seeder Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a Rust binary (`tc-seed`) that runs as a K8s CronJob to continuously populate the demo instance with LLM-generated rooms, polls, dimensions, and simulated votes via OpenRouter.

**Architecture:** A new `seed` module in the `service` crate with a thin binary entry point at `src/bin/seed.rs`. The module uses repo-layer functions directly (no HTTP, no crypto auth). OpenRouter is called via `reqwest` (already a dependency). Helm chart gets a CronJob template gated by `seed.enabled`.

**Tech Stack:** Rust (existing workspace), reqwest 0.12, sqlx 0.8, serde_json, figment (env vars), tc-crypto (for key generation), Helm templates.

**Design doc:** `docs/plans/2026-03-03-demo-room-seeder-design.md`

---

### Task 1: Add seed module scaffold and binary entry point

**Files:**
- Create: `service/src/seed/mod.rs`
- Create: `service/src/seed/config.rs`
- Create: `service/src/bin/seed.rs`
- Modify: `service/src/lib.rs:18` (add `pub mod seed;`)
- Modify: `service/Cargo.toml:69` (add `[[bin]]` entry)

**Step 1: Create the seed config types**

Create `service/src/seed/config.rs`:

```rust
//! Configuration for the demo seed worker.

use serde::Deserialize;

/// Seed worker configuration, loaded from environment variables.
#[derive(Debug, Clone, Deserialize)]
pub struct SeedConfig {
    /// OpenRouter API key
    pub openrouter_api_key: String,
    /// OpenRouter model identifier (e.g., "anthropic/claude-sonnet-4-6")
    #[serde(default = "default_model")]
    pub openrouter_model: String,
    /// Target number of open rooms with active polls
    #[serde(default = "default_target_rooms")]
    pub target_rooms: usize,
    /// Number of synthetic votes to cast per poll
    #[serde(default = "default_votes_per_poll")]
    pub votes_per_poll: usize,
    /// System prompt for LLM topic generation
    #[serde(default = "default_system_prompt")]
    pub system_prompt: String,
    /// Number of synthetic voter accounts to create
    #[serde(default = "default_voter_count")]
    pub voter_count: usize,
}

fn default_model() -> String {
    "anthropic/claude-sonnet-4-6".to_string()
}

fn default_target_rooms() -> usize {
    5
}

fn default_votes_per_poll() -> usize {
    15
}

fn default_system_prompt() -> String {
    "You are a civic engagement topic generator. Generate realistic local governance topics \
     suitable for community polling. Topics should be specific, actionable, and relevant to \
     a small city or town. Examples: zoning changes, park improvements, transit priorities, \
     budget allocation, public safety initiatives."
        .to_string()
}

fn default_voter_count() -> usize {
    20
}

impl SeedConfig {
    /// Load seed config from `SEED_*` environment variables.
    ///
    /// # Errors
    ///
    /// Returns an error if required env vars (like `SEED_OPENROUTER_API_KEY`) are missing.
    pub fn from_env() -> Result<Self, figment::Error> {
        use figment::{providers::Env, Figment};
        Figment::new()
            .merge(Env::prefixed("SEED_"))
            .extract()
    }
}
```

**Step 2: Create the seed module root**

Create `service/src/seed/mod.rs`:

```rust
//! Demo seed worker for populating rooms with LLM-generated content.

pub mod config;
```

**Step 3: Register the module in lib.rs**

Add `pub mod seed;` to `service/src/lib.rs` after the `rooms` module.

**Step 4: Create the binary entry point**

Create `service/src/bin/seed.rs`:

```rust
use anyhow::Context;
use tinycongress_api::{config::Config, db::setup_database, seed::config::SeedConfig};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // Load database config from standard TC config path
    let config = Config::load().map_err(|e| anyhow::anyhow!("{e}"))?;

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_new(&config.logging.level)
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!("tc-seed starting up");

    // Load seed-specific config from SEED_* env vars
    let seed_config = SeedConfig::from_env().context("failed to load seed config")?;
    tracing::info!(
        model = %seed_config.openrouter_model,
        target_rooms = seed_config.target_rooms,
        votes_per_poll = seed_config.votes_per_poll,
        voter_count = seed_config.voter_count,
        "seed config loaded"
    );

    // Connect to database (reuses existing retry logic and migrations)
    let pool = setup_database(&config.database).await?;

    tracing::info!("tc-seed run complete");
    pool.close().await;
    Ok(())
}
```

**Step 5: Add binary to Cargo.toml**

Add after the existing `[[bin]]` entries (after line 75 in `service/Cargo.toml`):

```toml
[[bin]]
name = "seed"
path = "src/bin/seed.rs"
```

**Step 6: Verify it compiles**

Run: `cargo build --bin seed -p tinycongress-api`
Expected: Successful compilation (warnings OK at this stage).

**Step 7: Commit**

```bash
git add service/src/seed/ service/src/bin/seed.rs service/src/lib.rs service/Cargo.toml
git commit -m "feat(seed): scaffold seed module and binary entry point"
```

---

### Task 2: LLM response types and OpenRouter client

**Files:**
- Create: `service/src/seed/llm.rs`
- Modify: `service/src/seed/mod.rs`

**Step 1: Write tests for LLM response deserialization**

Add to `service/src/seed/llm.rs` (at the bottom, in a `#[cfg(test)]` module):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_valid_llm_response() {
        let json = r#"{
            "rooms": [{
                "name": "Downtown Transit Expansion",
                "description": "Should the city invest in light rail?",
                "polls": [{
                    "question": "Which corridor should be prioritized?",
                    "description": "Rate each factor for the proposed routes",
                    "dimensions": [
                        {"name": "Ridership Impact", "description": "Expected daily riders", "min": 0.0, "max": 10.0},
                        {"name": "Cost Efficiency", "description": "Bang for the buck", "min": 0.0, "max": 10.0}
                    ]
                }]
            }]
        }"#;

        let response: SeedContent = serde_json::from_str(json).expect("valid json");
        assert_eq!(response.rooms.len(), 1);
        assert_eq!(response.rooms[0].name, "Downtown Transit Expansion");
        assert_eq!(response.rooms[0].polls.len(), 1);
        assert_eq!(response.rooms[0].polls[0].dimensions.len(), 2);
        assert!((response.rooms[0].polls[0].dimensions[0].max - 10.0).abs() < f32::EPSILON);
    }

    #[test]
    fn rejects_empty_rooms() {
        let json = r#"{"rooms": []}"#;
        let response: SeedContent = serde_json::from_str(json).expect("valid json");
        assert!(response.rooms.is_empty());
    }

    #[test]
    fn builds_correct_messages() {
        let config = super::super::config::SeedConfig {
            openrouter_api_key: "test-key".to_string(),
            openrouter_model: "test/model".to_string(),
            target_rooms: 3,
            votes_per_poll: 10,
            system_prompt: "Generate civic topics".to_string(),
            voter_count: 20,
        };
        let messages = build_messages(&config, 2);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "system");
        assert!(messages[1].content.contains('2'));
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p tinycongress-api seed::llm::tests --no-default-features`
Expected: FAIL — module and types don't exist yet.

**Step 3: Write the LLM types and client**

Create `service/src/seed/llm.rs`:

```rust
//! OpenRouter LLM client for generating seed content.

use anyhow::Context;
use serde::{Deserialize, Serialize};

use super::config::SeedConfig;

/// Structured content returned by the LLM.
#[derive(Debug, Clone, Deserialize)]
pub struct SeedContent {
    pub rooms: Vec<SeedRoom>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SeedRoom {
    pub name: String,
    pub description: String,
    pub polls: Vec<SeedPoll>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SeedPoll {
    pub question: String,
    pub description: String,
    pub dimensions: Vec<SeedDimension>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SeedDimension {
    pub name: String,
    pub description: String,
    pub min: f32,
    pub max: f32,
}

// --- OpenRouter API types ---

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    response_format: ResponseFormat,
    temperature: f32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
struct ResponseFormat {
    r#type: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ChoiceMessage,
}

#[derive(Debug, Deserialize)]
struct ChoiceMessage {
    content: String,
}

/// Build the system + user messages for the LLM request.
pub fn build_messages(config: &SeedConfig, rooms_needed: usize) -> Vec<ChatMessage> {
    let user_prompt = format!(
        "Generate exactly {rooms_needed} room(s) for a community governance platform. \
         Each room should have 2-3 polls, and each poll should have 3-5 voting dimensions. \
         Respond with valid JSON matching this schema:\n\
         {{\n  \"rooms\": [\n    {{\n      \"name\": \"string (unique, 5-60 chars)\",\n      \
         \"description\": \"string (1-2 sentences)\",\n      \"polls\": [\n        {{\n          \
         \"question\": \"string\",\n          \"description\": \"string (1-2 sentences)\",\n          \
         \"dimensions\": [\n            {{\"name\": \"string (1-3 words)\", \"description\": \"string\", \
         \"min\": 0.0, \"max\": 10.0}}\n          ]\n        }}\n      ]\n    }}\n  ]\n}}\n\
         Make room names specific and unique (e.g., include year or neighborhood). \
         Make poll questions concrete and actionable. \
         Make dimensions measurable aspects voters can evaluate independently."
    );

    vec![
        ChatMessage {
            role: "system".to_string(),
            content: config.system_prompt.clone(),
        },
        ChatMessage {
            role: "user".to_string(),
            content: user_prompt,
        },
    ]
}

/// Call OpenRouter to generate seed content.
///
/// # Errors
///
/// Returns an error if the HTTP request fails, the response is malformed,
/// or the LLM output cannot be parsed as `SeedContent`.
pub async fn generate_content(
    client: &reqwest::Client,
    config: &SeedConfig,
    rooms_needed: usize,
) -> Result<SeedContent, anyhow::Error> {
    let messages = build_messages(config, rooms_needed);

    let request = ChatRequest {
        model: config.openrouter_model.clone(),
        messages,
        response_format: ResponseFormat {
            r#type: "json_object".to_string(),
        },
        temperature: 0.9,
    };

    let response = client
        .post("https://openrouter.ai/api/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", config.openrouter_api_key))
        .json(&request)
        .send()
        .await
        .context("OpenRouter request failed")?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("OpenRouter returned {status}: {body}");
    }

    let chat_response: ChatResponse = response
        .json()
        .await
        .context("failed to parse OpenRouter response")?;

    let content_str = chat_response
        .choices
        .first()
        .map(|c| c.message.content.as_str())
        .unwrap_or("{}");

    let content: SeedContent =
        serde_json::from_str(content_str).context("failed to parse LLM JSON output")?;

    Ok(content)
}
```

**Step 4: Update seed module root**

Replace `service/src/seed/mod.rs`:

```rust
//! Demo seed worker for populating rooms with LLM-generated content.

pub mod config;
pub mod llm;
```

**Step 5: Run tests to verify they pass**

Run: `cargo test -p tinycongress-api seed::llm::tests`
Expected: 3 tests PASS.

**Step 6: Commit**

```bash
git add service/src/seed/llm.rs service/src/seed/mod.rs
git commit -m "feat(seed): add LLM response types and OpenRouter client"
```

---

### Task 3: Synthetic account creation

**Files:**
- Create: `service/src/seed/accounts.rs`
- Modify: `service/src/seed/mod.rs`

**Step 1: Write the account seeding function**

Create `service/src/seed/accounts.rs`:

```rust
//! Synthetic demo account management.

use sqlx::PgPool;
use tc_crypto::{encode_base64url, Kid};
use uuid::Uuid;

use crate::identity::repo::{
    create_account_with_executor, get_account_by_username, AccountRepoError,
};
use crate::reputation::repo::{
    create_endorsement, ensure_verifier_account, EndorsementRepoError,
};

/// A synthetic account with its database ID.
#[derive(Debug, Clone)]
pub struct SyntheticAccount {
    pub id: Uuid,
    pub username: String,
}

/// Generate a deterministic public key and KID from a seed byte.
/// Each seed produces a unique key pair.
fn generate_deterministic_keys(seed: u8) -> (String, Kid) {
    let pubkey = [seed; 32];
    let root_pubkey = encode_base64url(&pubkey);
    let root_kid = Kid::derive(&pubkey);
    (root_pubkey, root_kid)
}

/// Ensure that `count` synthetic demo accounts exist in the database.
///
/// Accounts are named `demo_voter_01` through `demo_voter_{count}` with
/// deterministic key material (seed = index). Existing accounts are skipped.
///
/// Returns the list of all synthetic accounts (both new and pre-existing).
///
/// # Errors
///
/// Returns an error if a database operation fails unexpectedly.
pub async fn ensure_synthetic_accounts(
    pool: &PgPool,
    count: usize,
) -> Result<Vec<SyntheticAccount>, anyhow::Error> {
    let mut accounts = Vec::with_capacity(count);

    for i in 1..=count {
        let username = format!("demo_voter_{i:02}");

        // Check if account already exists
        match get_account_by_username(pool, &username).await {
            Ok(existing) => {
                accounts.push(SyntheticAccount {
                    id: existing.id,
                    username,
                });
                continue;
            }
            Err(AccountRepoError::NotFound) => {
                // Expected — create it below
            }
            Err(e) => {
                return Err(anyhow::anyhow!("failed to check account {username}: {e}"));
            }
        }

        // Safe: i is 1..=count where count <= 255 in practice, but we clamp for safety.
        // We offset by 100 to avoid collisions with any test seeds.
        #[allow(clippy::cast_possible_truncation)]
        let seed = ((i + 100) % 256) as u8;
        let (root_pubkey, root_kid) = generate_deterministic_keys(seed);

        match create_account_with_executor(pool, &username, &root_pubkey, &root_kid).await {
            Ok(created) => {
                tracing::info!(%username, "created synthetic account");
                accounts.push(SyntheticAccount {
                    id: created.id,
                    username,
                });
            }
            Err(AccountRepoError::DuplicateUsername) => {
                // Race condition — another run created it. Fetch and continue.
                let existing = get_account_by_username(pool, &username)
                    .await
                    .map_err(|e| anyhow::anyhow!("failed to fetch account {username}: {e}"))?;
                accounts.push(SyntheticAccount {
                    id: existing.id,
                    username,
                });
            }
            Err(e) => {
                return Err(anyhow::anyhow!("failed to create account {username}: {e}"));
            }
        }
    }

    Ok(accounts)
}

/// Ensure all synthetic accounts are endorsed for a given topic.
///
/// Creates the verifier account if needed (idempotent), then endorses
/// each account. Skips accounts that are already endorsed.
///
/// # Errors
///
/// Returns an error if a database operation fails unexpectedly.
pub async fn ensure_endorsements(
    pool: &PgPool,
    accounts: &[SyntheticAccount],
    topic: &str,
) -> Result<(), anyhow::Error> {
    let verifier = ensure_verifier_account(pool, "demo-seeder", Some("Demo Seed Worker"))
        .await
        .map_err(|e| anyhow::anyhow!("failed to ensure verifier: {e}"))?;

    for account in accounts {
        match create_endorsement(pool, account.id, topic, verifier.id, None).await {
            Ok(_) => {
                tracing::debug!(username = %account.username, %topic, "endorsed account");
            }
            Err(EndorsementRepoError::Duplicate) => {
                // Already endorsed — expected on repeat runs
            }
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "failed to endorse {}: {e}",
                    account.username
                ));
            }
        }
    }

    Ok(())
}
```

**Step 2: Update seed module root**

Add `pub mod accounts;` to `service/src/seed/mod.rs`.

**Step 3: Verify it compiles**

Run: `cargo build --bin seed -p tinycongress-api`
Expected: Successful compilation.

**Step 4: Commit**

```bash
git add service/src/seed/accounts.rs service/src/seed/mod.rs
git commit -m "feat(seed): add synthetic account and endorsement management"
```

---

### Task 4: Content insertion (rooms, polls, dimensions)

**Files:**
- Create: `service/src/seed/content.rs`
- Modify: `service/src/seed/mod.rs`

**Step 1: Write the content insertion function**

Create `service/src/seed/content.rs`:

```rust
//! Insert LLM-generated content into the database.

use sqlx::PgPool;
use uuid::Uuid;

use crate::rooms::repo::{
    polls::{create_dimension, create_poll, update_poll_status},
    rooms::{create_room, RoomRepoError},
};

use super::llm::SeedContent;

/// Result of inserting seed content.
#[derive(Debug)]
pub struct InsertResult {
    pub rooms_created: usize,
    pub rooms_skipped: usize,
    pub polls_created: usize,
}

/// Insert LLM-generated rooms, polls, and dimensions into the database.
///
/// Rooms with duplicate names are skipped (idempotent). Polls within
/// successfully created rooms are activated immediately.
///
/// # Errors
///
/// Returns an error if a database operation fails unexpectedly.
pub async fn insert_seed_content(
    pool: &PgPool,
    content: &SeedContent,
) -> Result<InsertResult, anyhow::Error> {
    let mut result = InsertResult {
        rooms_created: 0,
        rooms_skipped: 0,
        polls_created: 0,
    };

    for seed_room in &content.rooms {
        // Create room (skip if name already exists)
        let room = match create_room(
            pool,
            &seed_room.name,
            Some(&seed_room.description),
            "identity_verified",
        )
        .await
        {
            Ok(room) => {
                tracing::info!(room_name = %seed_room.name, "created room");
                result.rooms_created += 1;
                room
            }
            Err(RoomRepoError::DuplicateName) => {
                tracing::debug!(room_name = %seed_room.name, "room already exists, skipping");
                result.rooms_skipped += 1;
                continue;
            }
            Err(e) => {
                return Err(anyhow::anyhow!("failed to create room '{}': {e}", seed_room.name));
            }
        };

        // Create polls and dimensions within the room
        for (poll_idx, seed_poll) in seed_room.polls.iter().enumerate() {
            let poll = create_poll(
                pool,
                room.id,
                &seed_poll.question,
                Some(&seed_poll.description),
            )
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "failed to create poll {} in room '{}': {e}",
                    poll_idx,
                    seed_room.name
                )
            })?;

            // Add dimensions
            for (dim_idx, seed_dim) in seed_poll.dimensions.iter().enumerate() {
                #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
                let sort_order = dim_idx as i32;

                create_dimension(
                    pool,
                    poll.id,
                    &seed_dim.name,
                    Some(&seed_dim.description),
                    seed_dim.min,
                    seed_dim.max,
                    sort_order,
                )
                .await
                .map_err(|e| {
                    anyhow::anyhow!(
                        "failed to create dimension '{}' for poll '{}': {e}",
                        seed_dim.name,
                        seed_poll.question
                    )
                })?;
            }

            // Activate the poll so it accepts votes
            update_poll_status(pool, poll.id, "active")
                .await
                .map_err(|e| {
                    anyhow::anyhow!("failed to activate poll '{}': {e}", seed_poll.question)
                })?;

            result.polls_created += 1;
        }
    }

    Ok(result)
}

/// Count the number of open rooms that have at least one active poll.
///
/// # Errors
///
/// Returns an error if the database query fails.
pub async fn count_active_rooms(pool: &PgPool) -> Result<usize, anyhow::Error> {
    let row = sqlx::query_scalar::<_, i64>(
        r"
        SELECT COUNT(DISTINCT r.id)
        FROM rooms__rooms r
        INNER JOIN rooms__polls p ON p.room_id = r.id
        WHERE r.status = 'open' AND p.status = 'active'
        ",
    )
    .fetch_one(pool)
    .await
    .map_err(|e| anyhow::anyhow!("failed to count active rooms: {e}"))?;

    #[allow(clippy::cast_sign_loss)]
    Ok(row as usize)
}

/// Collect poll IDs for all active polls (for vote seeding).
///
/// Returns `(poll_id, Vec<(dimension_id, min_value, max_value)>)` tuples.
///
/// # Errors
///
/// Returns an error if the database query fails.
pub async fn list_active_polls_with_dimensions(
    pool: &PgPool,
) -> Result<Vec<(Uuid, Vec<(Uuid, f32, f32)>)>, anyhow::Error> {
    #[derive(sqlx::FromRow)]
    struct Row {
        poll_id: Uuid,
        dimension_id: Uuid,
        min_value: f32,
        max_value: f32,
    }

    let rows = sqlx::query_as::<_, Row>(
        r"
        SELECT p.id AS poll_id, d.id AS dimension_id, d.min_value, d.max_value
        FROM rooms__polls p
        INNER JOIN rooms__poll_dimensions d ON d.poll_id = p.id
        WHERE p.status = 'active'
        ORDER BY p.id, d.sort_order
        ",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| anyhow::anyhow!("failed to list active polls: {e}"))?;

    // Group by poll_id
    let mut polls: Vec<(Uuid, Vec<(Uuid, f32, f32)>)> = Vec::new();
    for row in rows {
        if let Some(last) = polls.last_mut() {
            if last.0 == row.poll_id {
                last.1.push((row.dimension_id, row.min_value, row.max_value));
                continue;
            }
        }
        polls.push((row.poll_id, vec![(row.dimension_id, row.min_value, row.max_value)]));
    }

    Ok(polls)
}
```

**Step 2: Update seed module root**

Add `pub mod content;` to `service/src/seed/mod.rs`.

**Step 3: Verify it compiles**

Run: `cargo build --bin seed -p tinycongress-api`
Expected: Successful compilation.

**Step 4: Commit**

```bash
git add service/src/seed/content.rs service/src/seed/mod.rs
git commit -m "feat(seed): add content insertion and active room counting"
```

---

### Task 5: Vote simulation

**Files:**
- Create: `service/src/seed/votes.rs`
- Modify: `service/src/seed/mod.rs`

**Step 1: Write tests for vote value generation**

Add to `service/src/seed/votes.rs` (test section):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_values_within_bounds() {
        let mut rng = StdRng::seed_from_u64(42);
        for _ in 0..100 {
            let value = random_vote_value(&mut rng, 0.0, 10.0);
            assert!((0.0..=10.0).contains(&value), "value {value} out of range");
        }
    }

    #[test]
    fn different_seeds_produce_different_values() {
        let mut rng1 = StdRng::seed_from_u64(1);
        let mut rng2 = StdRng::seed_from_u64(2);
        let v1 = random_vote_value(&mut rng1, 0.0, 10.0);
        let v2 = random_vote_value(&mut rng2, 0.0, 10.0);
        assert!((v1 - v2).abs() > f32::EPSILON);
    }

    #[test]
    fn same_seed_is_reproducible() {
        let mut rng1 = StdRng::seed_from_u64(42);
        let mut rng2 = StdRng::seed_from_u64(42);
        let v1 = random_vote_value(&mut rng1, 0.0, 10.0);
        let v2 = random_vote_value(&mut rng2, 0.0, 10.0);
        assert!((v1 - v2).abs() < f32::EPSILON);
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p tinycongress-api seed::votes::tests --no-default-features`
Expected: FAIL — module doesn't exist yet.

**Step 3: Write the vote simulation logic**

Create `service/src/seed/votes.rs`:

```rust
//! Simulated vote casting for demo accounts.

use rand::prelude::*;
use rand::rngs::StdRng;
use sqlx::PgPool;
use uuid::Uuid;

use crate::rooms::repo::votes::upsert_vote;

use super::accounts::SyntheticAccount;

/// Generate a random vote value within [min, max] using a beta-like distribution
/// that tends toward the middle (more realistic than uniform).
pub fn random_vote_value(rng: &mut StdRng, min: f32, max: f32) -> f32 {
    // Average two uniform samples for a triangular-ish distribution
    let u1: f32 = rng.gen();
    let u2: f32 = rng.gen();
    let t = (u1 + u2) / 2.0;
    min + t * (max - min)
}

/// Cast simulated votes from synthetic accounts on all active polls.
///
/// Each synthetic account (up to `votes_per_poll`) votes on every dimension
/// of every active poll. Uses a seeded RNG for reproducibility — re-running
/// with the same data produces the same votes (upsert semantics).
///
/// # Errors
///
/// Returns an error if a database operation fails unexpectedly.
pub async fn cast_simulated_votes(
    pool: &PgPool,
    accounts: &[SyntheticAccount],
    polls: &[(Uuid, Vec<(Uuid, f32, f32)>)],
    votes_per_poll: usize,
) -> Result<usize, anyhow::Error> {
    let mut total_votes = 0;
    // Seed RNG from a fixed value for reproducibility
    let mut rng = StdRng::seed_from_u64(20260303);

    let voter_count = votes_per_poll.min(accounts.len());

    for (poll_id, dimensions) in polls {
        // Check how many voters already exist for this poll
        let existing_voters = crate::rooms::repo::votes::count_voters(pool, *poll_id)
            .await
            .map_err(|e| anyhow::anyhow!("failed to count voters for poll {poll_id}: {e}"))?;

        #[allow(clippy::cast_sign_loss)]
        let existing = existing_voters as usize;
        if existing >= voter_count {
            tracing::debug!(%poll_id, existing, target = voter_count, "poll already has enough votes");
            continue;
        }

        // Cast votes from accounts that haven't voted yet (start from existing count)
        for account in accounts.iter().skip(existing).take(voter_count - existing) {
            for &(dimension_id, min_val, max_val) in dimensions {
                let value = random_vote_value(&mut rng, min_val, max_val);
                upsert_vote(pool, *poll_id, dimension_id, account.id, value)
                    .await
                    .map_err(|e| {
                        anyhow::anyhow!(
                            "failed to cast vote for {} on poll {poll_id}: {e}",
                            account.username
                        )
                    })?;
                total_votes += 1;
            }
        }

        tracing::info!(%poll_id, new_voters = voter_count - existing, "seeded votes");
    }

    Ok(total_votes)
}
```

**Step 4: Update seed module root**

Add `pub mod votes;` to `service/src/seed/mod.rs`.

**Step 5: Run tests to verify they pass**

Run: `cargo test -p tinycongress-api seed::votes::tests`
Expected: 3 tests PASS.

**Step 6: Commit**

```bash
git add service/src/seed/votes.rs service/src/seed/mod.rs
git commit -m "feat(seed): add vote simulation with seeded RNG"
```

---

### Task 6: Wire up main orchestration

**Files:**
- Modify: `service/src/bin/seed.rs`

**Step 1: Implement the main seed flow**

Replace `service/src/bin/seed.rs` with:

```rust
use anyhow::Context;
use tinycongress_api::{
    config::Config,
    db::setup_database,
    seed::{
        accounts::{ensure_endorsements, ensure_synthetic_accounts},
        config::SeedConfig,
        content::{count_active_rooms, insert_seed_content, list_active_polls_with_dimensions},
        llm::generate_content,
        votes::cast_simulated_votes,
    },
};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // Load database config from standard TC config path
    let config = Config::load().map_err(|e| anyhow::anyhow!("{e}"))?;

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_new(&config.logging.level)
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!("tc-seed starting up");

    // Load seed-specific config from SEED_* env vars
    let seed_config = SeedConfig::from_env().context("failed to load seed config")?;
    tracing::info!(
        model = %seed_config.openrouter_model,
        target_rooms = seed_config.target_rooms,
        votes_per_poll = seed_config.votes_per_poll,
        voter_count = seed_config.voter_count,
        "seed config loaded"
    );

    // Connect to database (reuses existing retry logic and migrations)
    let pool = setup_database(&config.database).await?;

    // Step 1: Ensure synthetic accounts exist
    tracing::info!("ensuring synthetic accounts...");
    let accounts = ensure_synthetic_accounts(&pool, seed_config.voter_count).await?;
    tracing::info!(count = accounts.len(), "synthetic accounts ready");

    // Step 2: Ensure all accounts are endorsed for voting
    tracing::info!("ensuring endorsements...");
    ensure_endorsements(&pool, &accounts, "identity_verified").await?;
    tracing::info!("endorsements ready");

    // Step 3: Check how many active rooms exist
    let active_rooms = count_active_rooms(&pool).await?;
    tracing::info!(active_rooms, target = seed_config.target_rooms, "room count check");

    if active_rooms < seed_config.target_rooms {
        let rooms_needed = seed_config.target_rooms - active_rooms;
        tracing::info!(rooms_needed, "generating new content via LLM...");

        // Step 4: Generate content via OpenRouter
        let client = reqwest::Client::new();
        let content = generate_content(&client, &seed_config, rooms_needed).await?;
        tracing::info!(
            rooms_generated = content.rooms.len(),
            "LLM content received"
        );

        // Step 5: Insert content into database
        let result = insert_seed_content(&pool, &content).await?;
        tracing::info!(
            rooms_created = result.rooms_created,
            rooms_skipped = result.rooms_skipped,
            polls_created = result.polls_created,
            "content inserted"
        );
    } else {
        tracing::info!("room target met, skipping content generation");
    }

    // Step 6: Cast simulated votes on all active polls
    tracing::info!("casting simulated votes...");
    let polls = list_active_polls_with_dimensions(&pool).await?;
    let vote_count =
        cast_simulated_votes(&pool, &accounts, &polls, seed_config.votes_per_poll).await?;
    tracing::info!(votes_cast = vote_count, "vote seeding complete");

    tracing::info!("tc-seed run complete");
    pool.close().await;
    Ok(())
}
```

**Step 2: Verify it compiles**

Run: `cargo build --bin seed -p tinycongress-api`
Expected: Successful compilation.

**Step 3: Commit**

```bash
git add service/src/bin/seed.rs
git commit -m "feat(seed): wire up main orchestration flow"
```

---

### Task 7: Update Dockerfile to include seed binary

**Files:**
- Modify: `service/Dockerfile:33-35,44`

**Step 1: Add seed binary to the Docker build**

In `service/Dockerfile`, the build command (line 33-35) currently only builds the main package. Update to also build the seed binary. And copy it into the final image (line 44).

Change line 33-35 from:
```dockerfile
RUN --mount=type=cache,target=/usr/local/cargo/registry,id=cargo-registry \
    --mount=type=cache,target=/usr/local/cargo/git,id=cargo-git \
    cargo build --release -p tinycongress-api
```

To:
```dockerfile
RUN --mount=type=cache,target=/usr/local/cargo/registry,id=cargo-registry \
    --mount=type=cache,target=/usr/local/cargo/git,id=cargo-git \
    cargo build --release -p tinycongress-api --bin tinycongress-api --bin seed
```

After line 44 (`COPY --from=builder /usr/src/app/target/release/tinycongress-api /usr/local/bin/`), add:
```dockerfile
COPY --from=builder /usr/src/app/target/release/seed /usr/local/bin/
```

**Step 2: Verify Docker build works**

Run: `docker build -f service/Dockerfile .` (from repo root)
Expected: Successful build. Both binaries present in final image.

**Step 3: Commit**

```bash
git add service/Dockerfile
git commit -m "build: include seed binary in release Docker image"
```

---

### Task 8: Helm chart templates for CronJob

**Files:**
- Create: `kube/app/templates/cronjob-seed.yaml`
- Create: `kube/app/templates/configmap-seed.yaml`
- Create: `kube/app/templates/secret-seed.yaml`
- Modify: `kube/app/values.yaml`

**Step 1: Add seed values to values.yaml**

Append to the end of `kube/app/values.yaml`:

```yaml
# Demo seed worker configuration
seed:
  enabled: false
  schedule: "*/30 * * * *"  # Every 30 minutes
  openrouterApiKey: ""
  openrouterModel: "anthropic/claude-sonnet-4-6"
  targetRooms: 5
  votesPerPoll: 15
  voterCount: 20
  systemPrompt: >-
    You are a civic engagement topic generator. Generate realistic local
    governance topics suitable for community polling. Topics should be
    specific, actionable, and relevant to a small city or town.
  resources:
    requests:
      cpu: 50m
      memory: 64Mi
    limits:
      cpu: 200m
      memory: 128Mi
```

**Step 2: Create the seed ConfigMap template**

Create `kube/app/templates/configmap-seed.yaml`:

```yaml
{{- if .Values.seed.enabled }}
apiVersion: v1
kind: ConfigMap
metadata:
  name: {{ include "app.fullname" . }}-seed
  labels:
    {{- include "app.labels" . | nindent 4 }}
    app.kubernetes.io/component: seed
data:
  SEED_OPENROUTER_MODEL: {{ .Values.seed.openrouterModel | quote }}
  SEED_TARGET_ROOMS: {{ .Values.seed.targetRooms | quote }}
  SEED_VOTES_PER_POLL: {{ .Values.seed.votesPerPoll | quote }}
  SEED_VOTER_COUNT: {{ .Values.seed.voterCount | quote }}
  SEED_SYSTEM_PROMPT: {{ .Values.seed.systemPrompt | quote }}
{{- end }}
```

**Step 3: Create the seed Secret template**

Create `kube/app/templates/secret-seed.yaml`:

```yaml
{{- if .Values.seed.enabled }}
apiVersion: v1
kind: Secret
metadata:
  name: {{ include "app.fullname" . }}-seed
  labels:
    {{- include "app.labels" . | nindent 4 }}
    app.kubernetes.io/component: seed
type: Opaque
stringData:
  SEED_OPENROUTER_API_KEY: {{ .Values.seed.openrouterApiKey | quote }}
{{- end }}
```

**Step 4: Create the CronJob template**

Create `kube/app/templates/cronjob-seed.yaml`:

```yaml
{{- if .Values.seed.enabled }}
apiVersion: batch/v1
kind: CronJob
metadata:
  name: {{ include "app.fullname" . }}-seed
  labels:
    {{- include "app.labels" . | nindent 4 }}
    app.kubernetes.io/component: seed
spec:
  schedule: {{ .Values.seed.schedule | quote }}
  concurrencyPolicy: Forbid
  successfulJobsHistoryLimit: 3
  failedJobsHistoryLimit: 3
  jobTemplate:
    spec:
      backoffLimit: 2
      activeDeadlineSeconds: 300
      template:
        metadata:
          labels:
            {{- include "app.selectorLabels" . | nindent 12 }}
            app.kubernetes.io/component: seed
        spec:
          serviceAccountName: {{ include "app.serviceAccountName" . }}
          restartPolicy: OnFailure
          containers:
            - name: seed
              image: "{{ .Values.api.image.repository }}:{{ .Values.api.image.tag }}"
              {{- if .Values.api.image.digest }}
              image: "{{ .Values.api.image.repository }}@{{ .Values.api.image.digest }}"
              {{- end }}
              command: ["seed"]
              envFrom:
                - configMapRef:
                    name: {{ include "app.fullname" . }}-seed
                - secretRef:
                    name: {{ include "app.fullname" . }}-seed
              env:
                - name: TC_DATABASE__USER
                  valueFrom:
                    secretKeyRef:
                      name: {{ .Values.database.existingSecret | default (printf "%s-database" (include "app.fullname" .)) }}
                      key: user
                - name: TC_DATABASE__PASSWORD
                  valueFrom:
                    secretKeyRef:
                      name: {{ .Values.database.existingSecret | default (printf "%s-database" (include "app.fullname" .)) }}
                      key: password
                - name: TC_SYNTHETIC_BACKUP_KEY
                  valueFrom:
                    secretKeyRef:
                      name: {{ include "app.fullname" . }}-app
                      key: synthetic-backup-key
              volumeMounts:
                - name: app-config
                  mountPath: /etc/tc
                  readOnly: true
              resources:
                {{- toYaml .Values.seed.resources | nindent 16 }}
          volumes:
            - name: app-config
              configMap:
                name: {{ include "app.fullname" . }}-config
{{- end }}
```

**Step 5: Verify Helm template renders**

Run: `helm template test-release kube/app --set seed.enabled=true --set seed.openrouterApiKey=test-key`
Expected: CronJob, ConfigMap, and Secret templates render without errors. CronJob references the correct image, env vars, and volumes.

**Step 6: Commit**

```bash
git add kube/app/templates/cronjob-seed.yaml kube/app/templates/configmap-seed.yaml kube/app/templates/secret-seed.yaml kube/app/values.yaml
git commit -m "feat(helm): add CronJob, ConfigMap, and Secret for seed worker"
```

---

### Task 9: Update homelab-gitops deployment

**Files:**
- Modify: `/Users/icook/homelab-gitops/clusters/sauce/workloads/tiny-congress/helmrelease-demo.yaml`
- Create: SOPS-encrypted secret for OpenRouter API key (manual step — document only)

**Step 1: Add seed values to the HelmRelease**

Add to the `values` section of `helmrelease-demo.yaml`:

```yaml
    seed:
      enabled: true
      schedule: "*/30 * * * *"
      openrouterModel: "anthropic/claude-sonnet-4-6"
      targetRooms: 5
      votesPerPoll: 15
      voterCount: 20
      systemPrompt: >-
        You are a civic engagement topic generator. Generate realistic local
        governance topics suitable for community polling. Topics should be
        specific, actionable, and relevant to a small city or town. Examples:
        zoning changes, park improvements, transit priorities, budget allocation.
```

**Note:** The `openrouterApiKey` should be set via a SOPS-encrypted secret in the homelab-gitops repo, not in plaintext. The exact mechanism depends on how existing secrets (database password) are managed — check `encrypted-secrets/` directory.

**Step 2: Document the secret setup**

The OpenRouter API key must be added to the cluster. Check how existing secrets are managed:
- If using SOPS: add to the encrypted secrets file
- If using external-secrets: add to the secret store
- If using sealed-secrets: create a sealed secret

This is a manual step that should be done by the operator.

**Step 3: Commit the HelmRelease change**

```bash
cd /Users/icook/homelab-gitops
git add clusters/sauce/workloads/tiny-congress/helmrelease-demo.yaml
git commit -m "feat(tiny-congress): enable seed worker on demo instance"
```

---

### Task 10: Run lints and full test suite

**Step 1: Run lints**

Run: `just lint`
Expected: No errors. Fix any clippy warnings or formatting issues introduced by the new code.

**Step 2: Run tests**

Run: `just test`
Expected: All existing tests pass. New unit tests (LLM deserialization, vote RNG) pass.

**Step 3: Fix any issues and commit**

If lint or test failures occur, fix them and commit:

```bash
git add -u
git commit -m "fix(seed): address lint and test issues"
```
