# Room Engine Plugin Architecture — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Extract the rooms module into a plugin-based engine system so new room types can be authored independently.

**Architecture:** Platform (identity, trust, lifecycle) delegates to compiled-in Room Engines via a `RoomEngine` trait. The existing polling code becomes the first engine. Backend engines are separate Rust crates; frontend engines are directories under `web/src/engines/`. See `.plan/2026-03-18-room-engine-plugin-design.md` for the full design.

**Tech Stack:** Rust (Axum, sqlx, serde), React (TanStack Query, Mantine), PostgreSQL

**Key files to read before starting:**
- `.plan/2026-03-18-room-engine-plugin-design.md` — the design doc
- `service/src/rooms/` — current rooms module (will be extracted)
- `service/src/trust/constraints.rs` — constraint system (will be moved to shared lib)
- `service/src/trust/repo/mod.rs` — TrustRepo trait (will be adapted via TrustGraphReader)
- `service/src/main.rs` — app wiring (will gain EngineRegistry)
- `web/src/features/rooms/` — frontend rooms feature (will move to `web/src/engines/polling/`)
- `web/src/pages/Poll.page.tsx` — room detail page (will become thin EngineRouter shell)

---

## Phase 1: Create `tc-engine-api` crate

### Task 1: Scaffold the `tc-engine-api` crate

**Files:**
- Create: `crates/tc-engine-api/Cargo.toml`
- Create: `crates/tc-engine-api/src/lib.rs`
- Modify: `Cargo.toml` (workspace root)

**Step 1: Check existing crate structure**

Run: `ls crates/`
Observe current crates (should see `tc-crypto/`).

**Step 2: Create the crate**

```toml
# crates/tc-engine-api/Cargo.toml
[package]
name = "tc-engine-api"
version = "0.1.0"
edition = "2021"

[dependencies]
anyhow = "1"
axum = { version = "0.7", features = ["macros"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sqlx = { version = "0.8", features = ["runtime-tokio", "postgres", "uuid", "chrono"] }
tokio = { version = "1", features = ["rt"] }
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
```

```rust
// crates/tc-engine-api/src/lib.rs
pub mod constraints;
pub mod engine;
pub mod error;
pub mod trust;

pub use constraints::{ConstraintRegistry, Eligibility};
pub use engine::{EngineContext, EngineMetadata, PlatformState, RoomEngine};
pub use error::EngineError;
pub use trust::{TrustGraphReader, TrustScoreSnapshot};
```

**Step 3: Add to workspace**

Add `"crates/tc-engine-api"` to the `members` list in root `Cargo.toml`.

**Step 4: Verify it compiles**

Run: `cargo check -p tc-engine-api`
Expected: PASS (empty modules)

**Step 5: Commit**

```bash
git add crates/tc-engine-api/ Cargo.toml Cargo.lock
git commit -m "chore: scaffold tc-engine-api crate"
```

---

### Task 2: Define `TrustGraphReader` trait and `TrustScoreSnapshot`

**Files:**
- Create: `crates/tc-engine-api/src/trust.rs`

These mirror the actual methods that constraints call on `TrustRepo`:
- `get_score()` → returns composite `TrustScoreSnapshot` (used by `endorsed_by`, `community`, `congress`)
- `has_endorsement()` → takes `&[Uuid]` verifier IDs (used by `identity_verified`)

**Step 1: Write the trust module**

```rust
// crates/tc-engine-api/src/trust.rs
use uuid::Uuid;

/// Composite trust score for a subject relative to an anchor.
#[derive(Debug, Clone)]
pub struct TrustScoreSnapshot {
    pub trust_distance: f64,
    pub path_diversity: u32,
    pub eigenvector_centrality: f64,
}

/// Thin read-only view of the trust graph.
/// Defined here in tc-engine-api; implemented by the service crate
/// via delegation to its full TrustRepo.
#[allow(async_fn_in_trait)]
pub trait TrustGraphReader: Send + Sync {
    /// Returns the composite trust score for a subject relative to an anchor.
    async fn get_score(
        &self,
        subject: Uuid,
        anchor: Option<Uuid>,
    ) -> Result<Option<TrustScoreSnapshot>, anyhow::Error>;

    /// Check if a subject has an active endorsement from any of the given
    /// verifiers for the specified topic.
    async fn has_endorsement(
        &self,
        subject: Uuid,
        topic: &str,
        verifier_ids: &[Uuid],
    ) -> Result<bool, anyhow::Error>;
}
```

**Step 2: Verify it compiles**

Run: `cargo check -p tc-engine-api`
Expected: PASS

**Step 3: Commit**

```bash
git add crates/tc-engine-api/src/trust.rs
git commit -m "feat(tc-engine-api): define TrustGraphReader trait"
```

---

### Task 3: Define `EngineError`

**Files:**
- Create: `crates/tc-engine-api/src/error.rs`

Maps engine errors to HTTP status codes. This replaces the per-handler error mapping currently in `service/src/rooms/http/mod.rs`.

**Step 1: Write the error module**

```rust
// crates/tc-engine-api/src/error.rs
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// Standard error type for engine route handlers.
/// Engines return this from handlers; IntoResponse maps to HTTP status.
#[derive(Debug)]
pub enum EngineError {
    NotFound(String),
    NotEligible(String),
    InvalidInput(String),
    Conflict(String),
    Internal(anyhow::Error),
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

impl IntoResponse for EngineError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            Self::NotEligible(msg) => (StatusCode::FORBIDDEN, msg),
            Self::InvalidInput(msg) => (StatusCode::BAD_REQUEST, msg),
            Self::Conflict(msg) => (StatusCode::CONFLICT, msg),
            Self::Internal(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Internal error: {err}"),
            ),
        };
        (status, axum::Json(ErrorBody { error: message })).into_response()
    }
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(msg) => write!(f, "not found: {msg}"),
            Self::NotEligible(msg) => write!(f, "not eligible: {msg}"),
            Self::InvalidInput(msg) => write!(f, "invalid input: {msg}"),
            Self::Conflict(msg) => write!(f, "conflict: {msg}"),
            Self::Internal(err) => write!(f, "internal error: {err}"),
        }
    }
}

impl std::error::Error for EngineError {}
```

**Step 2: Verify it compiles**

Run: `cargo check -p tc-engine-api`
Expected: PASS

**Step 3: Commit**

```bash
git add crates/tc-engine-api/src/error.rs
git commit -m "feat(tc-engine-api): define EngineError with IntoResponse"
```

---

### Task 4: Define `Eligibility`, constraint trait, and `ConstraintRegistry`

**Files:**
- Create: `crates/tc-engine-api/src/constraints.rs`

Port the constraint system from `service/src/trust/constraints.rs`. The key change: constraints depend on `TrustGraphReader` instead of `TrustRepo`.

**Step 1: Write the constraints module**

```rust
// crates/tc-engine-api/src/constraints.rs
use std::sync::Arc;
use uuid::Uuid;

use crate::trust::TrustGraphReader;

/// Result of an eligibility check.
#[derive(Debug, Clone)]
pub struct Eligibility {
    pub is_eligible: bool,
    pub reason: Option<String>,
}

/// A constraint that determines whether a user can participate in a room.
#[allow(async_fn_in_trait)]
pub trait RoomConstraint: Send + Sync {
    async fn check(
        &self,
        user_id: Uuid,
        trust_reader: &dyn TrustGraphReader,
    ) -> Result<Eligibility, anyhow::Error>;
}

/// Factory: build a constraint from its type string and JSON config.
pub fn build_constraint(
    constraint_type: &str,
    config: &serde_json::Value,
) -> Result<Box<dyn RoomConstraint>, anyhow::Error> {
    match constraint_type {
        "endorsed_by" => {
            let anchor_id: Uuid = serde_json::from_value(
                config
                    .get("anchor_id")
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("missing anchor_id"))?,
            )?;
            Ok(Box::new(EndorsedByConstraint::new(anchor_id)))
        }
        "community" => {
            let anchor_id: Uuid = serde_json::from_value(
                config
                    .get("anchor_id")
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("missing anchor_id"))?,
            )?;
            let max_distance = config
                .get("max_distance")
                .and_then(|v| v.as_f64())
                .unwrap_or(5.0) as f32;
            let min_diversity = config
                .get("min_diversity")
                .and_then(|v| v.as_i64())
                .unwrap_or(2) as i32;
            Ok(Box::new(CommunityConstraint::new(
                anchor_id,
                max_distance,
                min_diversity,
            )?))
        }
        "congress" => {
            let anchor_id: Uuid = serde_json::from_value(
                config
                    .get("anchor_id")
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("missing anchor_id"))?,
            )?;
            let min_diversity = config
                .get("min_diversity")
                .and_then(|v| v.as_i64())
                .unwrap_or(3) as i32;
            Ok(Box::new(CongressConstraint::new(
                anchor_id,
                min_diversity,
            )?))
        }
        "identity_verified" => {
            let verifier_ids: Vec<Uuid> = serde_json::from_value(
                config
                    .get("verifier_ids")
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("missing verifier_ids"))?,
            )?;
            let topic = config
                .get("topic")
                .and_then(|v| v.as_str())
                .unwrap_or("identity_verified")
                .to_string();
            Ok(Box::new(IdentityVerifiedConstraint::new(
                verifier_ids,
                topic,
            )))
        }
        other => anyhow::bail!("unknown constraint type: {other}"),
    }
}

/// Wraps the shared constraint implementations.
/// Engines call check() on submission routes to gate participation.
pub struct ConstraintRegistry {
    trust_reader: Arc<dyn TrustGraphReader>,
}

impl ConstraintRegistry {
    pub fn new(trust_reader: Arc<dyn TrustGraphReader>) -> Self {
        Self { trust_reader }
    }

    /// Evaluate the given constraint against a user.
    pub async fn check(
        &self,
        constraint_type: &str,
        constraint_config: &serde_json::Value,
        user_id: Uuid,
    ) -> Result<Eligibility, anyhow::Error> {
        let constraint = build_constraint(constraint_type, constraint_config)?;
        constraint.check(user_id, self.trust_reader.as_ref()).await
    }
}

// --- Constraint implementations ---
// These are ported from service/src/trust/constraints.rs
// with TrustRepo replaced by TrustGraphReader.

pub struct EndorsedByConstraint {
    anchor_id: Uuid,
}

impl EndorsedByConstraint {
    pub const fn new(anchor_id: Uuid) -> Self {
        Self { anchor_id }
    }
}

impl RoomConstraint for EndorsedByConstraint {
    async fn check(
        &self,
        user_id: Uuid,
        trust_reader: &dyn TrustGraphReader,
    ) -> Result<Eligibility, anyhow::Error> {
        let score = trust_reader
            .get_score(user_id, Some(self.anchor_id))
            .await?;
        match score {
            Some(_) => Ok(Eligibility {
                is_eligible: true,
                reason: None,
            }),
            None => Ok(Eligibility {
                is_eligible: false,
                reason: Some("No trust path to anchor".to_string()),
            }),
        }
    }
}

pub struct CommunityConstraint {
    anchor_id: Uuid,
    max_distance: f32,
    min_diversity: i32,
}

impl CommunityConstraint {
    pub fn new(
        anchor_id: Uuid,
        max_distance: f32,
        min_diversity: i32,
    ) -> Result<Self, anyhow::Error> {
        anyhow::ensure!(max_distance > 0.0, "max_distance must be positive");
        anyhow::ensure!(min_diversity > 0, "min_diversity must be positive");
        Ok(Self {
            anchor_id,
            max_distance,
            min_diversity,
        })
    }
}

impl RoomConstraint for CommunityConstraint {
    async fn check(
        &self,
        user_id: Uuid,
        trust_reader: &dyn TrustGraphReader,
    ) -> Result<Eligibility, anyhow::Error> {
        let score = trust_reader
            .get_score(user_id, Some(self.anchor_id))
            .await?;
        match score {
            None => Ok(Eligibility {
                is_eligible: false,
                reason: Some("No trust score found".to_string()),
            }),
            Some(s) => {
                let distance_ok = s.trust_distance <= self.max_distance as f64;
                let diversity_ok = s.path_diversity >= self.min_diversity as u32;
                if distance_ok && diversity_ok {
                    Ok(Eligibility {
                        is_eligible: true,
                        reason: None,
                    })
                } else {
                    let mut reasons = Vec::new();
                    if !distance_ok {
                        reasons.push(format!(
                            "trust distance {:.2} exceeds max {:.2}",
                            s.trust_distance, self.max_distance
                        ));
                    }
                    if !diversity_ok {
                        reasons.push(format!(
                            "path diversity {} below min {}",
                            s.path_diversity, self.min_diversity
                        ));
                    }
                    Ok(Eligibility {
                        is_eligible: false,
                        reason: Some(reasons.join("; ")),
                    })
                }
            }
        }
    }
}

pub struct CongressConstraint {
    anchor_id: Uuid,
    min_diversity: i32,
}

impl CongressConstraint {
    pub fn new(anchor_id: Uuid, min_diversity: i32) -> Result<Self, anyhow::Error> {
        anyhow::ensure!(min_diversity > 0, "min_diversity must be positive");
        Ok(Self {
            anchor_id,
            min_diversity,
        })
    }
}

impl RoomConstraint for CongressConstraint {
    async fn check(
        &self,
        user_id: Uuid,
        trust_reader: &dyn TrustGraphReader,
    ) -> Result<Eligibility, anyhow::Error> {
        let score = trust_reader
            .get_score(user_id, Some(self.anchor_id))
            .await?;
        match score {
            None => Ok(Eligibility {
                is_eligible: false,
                reason: Some("No trust score found".to_string()),
            }),
            Some(s) => {
                if s.path_diversity >= self.min_diversity as u32 {
                    Ok(Eligibility {
                        is_eligible: true,
                        reason: None,
                    })
                } else {
                    Ok(Eligibility {
                        is_eligible: false,
                        reason: Some(format!(
                            "path diversity {} below min {}",
                            s.path_diversity, self.min_diversity
                        )),
                    })
                }
            }
        }
    }
}

pub struct IdentityVerifiedConstraint {
    verifier_ids: Vec<Uuid>,
    topic: String,
}

impl IdentityVerifiedConstraint {
    pub fn new(verifier_ids: Vec<Uuid>, topic: impl Into<String>) -> Self {
        Self {
            verifier_ids,
            topic: topic.into(),
        }
    }
}

impl RoomConstraint for IdentityVerifiedConstraint {
    async fn check(
        &self,
        user_id: Uuid,
        trust_reader: &dyn TrustGraphReader,
    ) -> Result<Eligibility, anyhow::Error> {
        let has = trust_reader
            .has_endorsement(user_id, &self.topic, &self.verifier_ids)
            .await?;
        if has {
            Ok(Eligibility {
                is_eligible: true,
                reason: None,
            })
        } else {
            Ok(Eligibility {
                is_eligible: false,
                reason: Some("Not endorsed by any approved verifier".to_string()),
            })
        }
    }
}
```

**Step 2: Verify it compiles**

Run: `cargo check -p tc-engine-api`
Expected: PASS

**Step 3: Commit**

```bash
git add crates/tc-engine-api/src/constraints.rs
git commit -m "feat(tc-engine-api): port constraint system to TrustGraphReader"
```

---

### Task 5: Write constraint unit tests

**Files:**
- Create: `crates/tc-engine-api/src/constraints_tests.rs` (or inline `#[cfg(test)]` module)

Test each constraint against a mock `TrustGraphReader`. These replace `service/tests/trust_constraint_tests.rs` (which tests against a real DB). The unit tests here verify the constraint logic; integration tests stay in `service/tests/`.

**Step 1: Write tests with a mock TrustGraphReader**

Add a `#[cfg(test)] mod tests` block at the bottom of `crates/tc-engine-api/src/constraints.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    struct MockTrustReader {
        score: Option<crate::trust::TrustScoreSnapshot>,
        has_endorsement: bool,
    }

    impl TrustGraphReader for MockTrustReader {
        async fn get_score(
            &self,
            _subject: Uuid,
            _anchor: Option<Uuid>,
        ) -> Result<Option<crate::trust::TrustScoreSnapshot>, anyhow::Error> {
            Ok(self.score.clone())
        }

        async fn has_endorsement(
            &self,
            _subject: Uuid,
            _topic: &str,
            _verifier_ids: &[Uuid],
        ) -> Result<bool, anyhow::Error> {
            Ok(self.has_endorsement)
        }
    }

    fn score(distance: f64, diversity: u32) -> Option<crate::trust::TrustScoreSnapshot> {
        Some(crate::trust::TrustScoreSnapshot {
            trust_distance: distance,
            path_diversity: diversity,
            eigenvector_centrality: 0.5,
        })
    }

    #[tokio::test]
    async fn endorsed_by_eligible_when_score_exists() {
        let reader = MockTrustReader { score: score(1.0, 3), has_endorsement: false };
        let constraint = EndorsedByConstraint::new(Uuid::new_v4());
        let result = constraint.check(Uuid::new_v4(), &reader).await.unwrap();
        assert!(result.is_eligible);
    }

    #[tokio::test]
    async fn endorsed_by_ineligible_when_no_score() {
        let reader = MockTrustReader { score: None, has_endorsement: false };
        let constraint = EndorsedByConstraint::new(Uuid::new_v4());
        let result = constraint.check(Uuid::new_v4(), &reader).await.unwrap();
        assert!(!result.is_eligible);
    }

    #[tokio::test]
    async fn community_eligible_when_within_bounds() {
        let reader = MockTrustReader { score: score(2.0, 3), has_endorsement: false };
        let constraint = CommunityConstraint::new(Uuid::new_v4(), 5.0, 2).unwrap();
        let result = constraint.check(Uuid::new_v4(), &reader).await.unwrap();
        assert!(result.is_eligible);
    }

    #[tokio::test]
    async fn community_ineligible_distance_exceeded() {
        let reader = MockTrustReader { score: score(6.0, 3), has_endorsement: false };
        let constraint = CommunityConstraint::new(Uuid::new_v4(), 5.0, 2).unwrap();
        let result = constraint.check(Uuid::new_v4(), &reader).await.unwrap();
        assert!(!result.is_eligible);
        assert!(result.reason.unwrap().contains("trust distance"));
    }

    #[tokio::test]
    async fn community_ineligible_diversity_below_min() {
        let reader = MockTrustReader { score: score(2.0, 1), has_endorsement: false };
        let constraint = CommunityConstraint::new(Uuid::new_v4(), 5.0, 2).unwrap();
        let result = constraint.check(Uuid::new_v4(), &reader).await.unwrap();
        assert!(!result.is_eligible);
        assert!(result.reason.unwrap().contains("path diversity"));
    }

    #[tokio::test]
    async fn congress_eligible_when_diversity_met() {
        let reader = MockTrustReader { score: score(10.0, 4), has_endorsement: false };
        let constraint = CongressConstraint::new(Uuid::new_v4(), 3).unwrap();
        let result = constraint.check(Uuid::new_v4(), &reader).await.unwrap();
        assert!(result.is_eligible);
    }

    #[tokio::test]
    async fn congress_ineligible_diversity_below() {
        let reader = MockTrustReader { score: score(10.0, 2), has_endorsement: false };
        let constraint = CongressConstraint::new(Uuid::new_v4(), 3).unwrap();
        let result = constraint.check(Uuid::new_v4(), &reader).await.unwrap();
        assert!(!result.is_eligible);
    }

    #[tokio::test]
    async fn identity_verified_eligible_when_endorsed() {
        let reader = MockTrustReader { score: None, has_endorsement: true };
        let constraint = IdentityVerifiedConstraint::new(vec![Uuid::new_v4()], "identity_verified");
        let result = constraint.check(Uuid::new_v4(), &reader).await.unwrap();
        assert!(result.is_eligible);
    }

    #[tokio::test]
    async fn identity_verified_ineligible_when_not_endorsed() {
        let reader = MockTrustReader { score: None, has_endorsement: false };
        let constraint = IdentityVerifiedConstraint::new(vec![Uuid::new_v4()], "identity_verified");
        let result = constraint.check(Uuid::new_v4(), &reader).await.unwrap();
        assert!(!result.is_eligible);
    }

    #[tokio::test]
    async fn build_constraint_endorsed_by() {
        let config = serde_json::json!({ "anchor_id": Uuid::new_v4() });
        let constraint = build_constraint("endorsed_by", &config).unwrap();
        let reader = MockTrustReader { score: score(1.0, 1), has_endorsement: false };
        let result = constraint.check(Uuid::new_v4(), &reader).await.unwrap();
        assert!(result.is_eligible);
    }

    #[tokio::test]
    async fn build_constraint_unknown_type_errors() {
        let result = build_constraint("unknown", &serde_json::json!({}));
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn constraint_registry_delegates_correctly() {
        let reader = Arc::new(MockTrustReader { score: score(1.0, 3), has_endorsement: false });
        let registry = ConstraintRegistry::new(reader);
        let config = serde_json::json!({ "anchor_id": Uuid::new_v4() });
        let result = registry.check("endorsed_by", &config, Uuid::new_v4()).await.unwrap();
        assert!(result.is_eligible);
    }
}
```

**Step 2: Run the tests**

Run: `cargo test -p tc-engine-api`
Expected: All tests PASS

**Step 3: Commit**

```bash
git add crates/tc-engine-api/src/constraints.rs
git commit -m "test(tc-engine-api): constraint unit tests with mock TrustGraphReader"
```

---

### Task 6: Define `RoomEngine` trait, `EngineContext`, `PlatformState`, `EngineMetadata`

**Files:**
- Create: `crates/tc-engine-api/src/engine.rs`

This is the core plugin trait. Read the design doc for the full specification.

**Step 1: Write the engine module**

```rust
// crates/tc-engine-api/src/engine.rs
use std::collections::HashMap;
use std::sync::Arc;

use sqlx::PgPool;
use uuid::Uuid;

use crate::constraints::ConstraintRegistry;
use crate::trust::TrustGraphReader;

/// Human-readable engine metadata.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EngineMetadata {
    pub display_name: String,
    pub description: String,
}

/// Room lifecycle operations — read room metadata, trigger lifecycle transitions.
/// Defined here; implemented by the service crate's platform layer.
#[allow(async_fn_in_trait)]
pub trait RoomLifecycle: Send + Sync {
    async fn get_room(&self, room_id: Uuid) -> Result<RoomRecord, anyhow::Error>;
    async fn close_room(&self, room_id: Uuid) -> Result<(), anyhow::Error>;
}

/// Minimal room record exposed to engines via RoomLifecycle.
/// Engines should not depend on all platform columns — only what they need.
#[derive(Debug, Clone)]
pub struct RoomRecord {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub status: String,
    pub engine_type: String,
    pub engine_config: serde_json::Value,
    pub constraint_type: String,
    pub constraint_config: serde_json::Value,
    pub poll_duration_secs: Option<i64>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub closed_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Everything an engine needs at runtime.
#[derive(Clone)]
pub struct EngineContext {
    pub pool: PgPool,
    pub trust_reader: Arc<dyn TrustGraphReader>,
    pub constraints: Arc<ConstraintRegistry>,
    pub room_lifecycle: Arc<dyn RoomLifecycle>,
}

/// Top-level Axum app state. Contains the engine registry and shared context.
#[derive(Clone)]
pub struct PlatformState {
    pub engine_registry: Arc<EngineRegistry>,
    pub engine_ctx: EngineContext,
}

/// The core plugin trait. Each room engine implements this.
#[allow(async_fn_in_trait)]
pub trait RoomEngine: Send + Sync + 'static {
    /// Unique string identifier, e.g. "polling", "pairwise"
    fn engine_type(&self) -> &str;

    /// Human-readable metadata
    fn metadata(&self) -> EngineMetadata;

    /// Axum router fragment. Platform mounts under /rooms/{room_id}/...
    fn routes(&self) -> axum::Router<PlatformState>;

    /// JSON schema describing engine-specific room configuration.
    fn config_schema(&self) -> serde_json::Value;

    /// Validate that a config blob is valid for this engine.
    fn validate_config(&self, config: &serde_json::Value) -> Result<(), crate::error::EngineError>;

    /// Called after a room instance is created. Initialize engine-specific state.
    async fn on_room_created(
        &self,
        room_id: Uuid,
        config: &serde_json::Value,
        ctx: &EngineContext,
    ) -> Result<(), crate::error::EngineError>;

    /// Spawn background tasks (lifecycle consumers, round managers, etc.)
    /// Called once at startup. Engine owns the join handles.
    fn start(&self, ctx: EngineContext) -> Result<Vec<tokio::task::JoinHandle<()>>, anyhow::Error>;
}

/// Registry of compiled-in engines. Lookup by engine_type string.
pub struct EngineRegistry {
    engines: HashMap<String, Arc<dyn RoomEngine>>,
}

impl EngineRegistry {
    pub fn new() -> Self {
        Self {
            engines: HashMap::new(),
        }
    }

    pub fn register(&mut self, engine: impl RoomEngine) {
        let key = engine.engine_type().to_string();
        self.engines.insert(key, Arc::new(engine));
    }

    pub fn get(&self, engine_type: &str) -> Option<Arc<dyn RoomEngine>> {
        self.engines.get(engine_type).cloned()
    }

    pub fn all(&self) -> Vec<Arc<dyn RoomEngine>> {
        self.engines.values().cloned().collect()
    }

    pub fn engine_types(&self) -> Vec<&str> {
        self.engines.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for EngineRegistry {
    fn default() -> Self {
        Self::new()
    }
}
```

**Step 2: Verify it compiles**

Run: `cargo check -p tc-engine-api`
Expected: PASS

**Step 3: Commit**

```bash
git add crates/tc-engine-api/src/engine.rs
git commit -m "feat(tc-engine-api): define RoomEngine trait, EngineContext, PlatformState, EngineRegistry"
```

---

### Task 7: Update `lib.rs` exports and verify full crate

**Files:**
- Modify: `crates/tc-engine-api/src/lib.rs`

**Step 1: Update lib.rs to export everything**

Verify the `lib.rs` written in Task 1 correctly re-exports all public types. Adjust if needed based on actual module structure.

**Step 2: Run full check + tests**

Run: `cargo check -p tc-engine-api && cargo test -p tc-engine-api`
Expected: All PASS

**Step 3: Run workspace-wide check**

Run: `cargo check`
Expected: PASS — tc-engine-api should not break any existing crate since nothing depends on it yet.

**Step 4: Commit if any lib.rs changes were needed**

---

## Phase 2: Implement `TrustGraphReader` adapter in the service crate

### Task 8: Implement `TrustGraphReader` for `PgTrustRepo`

**Files:**
- Modify: `service/Cargo.toml` — add `tc-engine-api` dependency
- Create: `service/src/trust/graph_reader.rs` — adapter implementation
- Modify: `service/src/trust/mod.rs` — add module

The service crate's `PgTrustRepo` already has `get_score()` and `has_identity_endorsement()`. This task writes a thin adapter that implements `TrustGraphReader` by delegating to `TrustRepo`.

**Step 1: Add tc-engine-api dependency to service**

Add to `service/Cargo.toml` under `[dependencies]`:
```toml
tc-engine-api = { path = "../crates/tc-engine-api" }
```

**Step 2: Write the adapter**

```rust
// service/src/trust/graph_reader.rs
use std::sync::Arc;
use tc_engine_api::trust::{TrustGraphReader, TrustScoreSnapshot};
use uuid::Uuid;

use super::repo::TrustRepo;

/// Adapts the full TrustRepo to the thin TrustGraphReader interface
/// that engines and constraints use.
pub struct TrustRepoGraphReader {
    trust_repo: Arc<dyn TrustRepo>,
}

impl TrustRepoGraphReader {
    pub fn new(trust_repo: Arc<dyn TrustRepo>) -> Self {
        Self { trust_repo }
    }
}

impl TrustGraphReader for TrustRepoGraphReader {
    async fn get_score(
        &self,
        subject: Uuid,
        anchor: Option<Uuid>,
    ) -> Result<Option<TrustScoreSnapshot>, anyhow::Error> {
        let score = self.trust_repo.get_score(subject, anchor).await?;
        Ok(score.map(|s| TrustScoreSnapshot {
            trust_distance: s.trust_distance,
            path_diversity: s.path_diversity as u32,
            eigenvector_centrality: s.eigenvector_centrality,
        }))
    }

    async fn has_endorsement(
        &self,
        subject: Uuid,
        topic: &str,
        verifier_ids: &[Uuid],
    ) -> Result<bool, anyhow::Error> {
        Ok(self
            .trust_repo
            .has_identity_endorsement(subject, verifier_ids, topic)
            .await?)
    }
}
```

**Note:** Check the actual field types on `ScoreSnapshot` in `service/src/trust/repo/mod.rs` before writing the mapping. The `path_diversity` field may be `i32` or `i64` — cast accordingly.

**Step 3: Register the module**

Add `pub mod graph_reader;` to `service/src/trust/mod.rs`.

**Step 4: Verify it compiles**

Run: `cargo check -p tc-service` (or whatever the service package name is — check `service/Cargo.toml`)
Expected: PASS

**Step 5: Commit**

```bash
git add service/Cargo.toml service/src/trust/graph_reader.rs service/src/trust/mod.rs
git commit -m "feat(service): implement TrustGraphReader adapter over TrustRepo"
```

---

## Phase 3: Wire up `EngineRegistry` in the service crate

This phase focuses on creating the platform routing layer and wiring the registry into the app, **without yet extracting the polling engine**. The polling code stays in `service/src/rooms/` for now — we just add the infrastructure to route through engines.

### Task 9: Add `engine_type` and `engine_config` columns

**Files:**
- Create: `service/migrations/<next_number>_engine_type.sql`

**Step 1: Check the latest migration number**

Run: `ls service/migrations/*.sql | sort -V | tail -1`

**Step 2: Write the migration**

```sql
-- Add engine_type and engine_config to rooms table.
-- All existing rooms are polling rooms.
ALTER TABLE rooms__rooms
  ADD COLUMN engine_type TEXT NOT NULL DEFAULT 'polling',
  ADD COLUMN engine_config JSONB NOT NULL DEFAULT '{}';
```

**Step 3: Commit**

```bash
git add service/migrations/
git commit -m "feat(db): add engine_type and engine_config to rooms table"
```

---

### Task 10: Create the engine router middleware

**Files:**
- Create: `service/src/engine_registry.rs`
- Modify: `service/src/lib.rs` — add module

This creates the Axum middleware/router layer that looks up `engine_type` for a room and delegates to the correct engine's routes.

**Step 1: Write the engine router**

This is where the platform-level routing logic lives. The key behavior:
1. Extract `room_id` from the path
2. Query the DB for the room's `engine_type`
3. Delegate to the matching engine's router
4. Inject `EngineContext` as an extension

Read `service/src/rooms/http/mod.rs` lines 189+ to understand how the current router is structured, then write a new `engine_router` function that wraps engine routes.

**Important:** At this stage, the polling engine routes are still in `service/src/rooms/http/`. The engine_router will initially just be infrastructure — the actual delegation happens in a later task.

**Step 2: Verify it compiles**

Run: `cargo check -p tc-service`

**Step 3: Commit**

```bash
git add service/src/engine_registry.rs service/src/lib.rs
git commit -m "feat(service): add engine registry and routing middleware"
```

---

### Task 11: Wire `EngineRegistry` into `main.rs`

**Files:**
- Modify: `service/src/main.rs`

**Step 1: Read current wiring**

Read `service/src/main.rs` to understand the current `build_app` function and how `rooms_service` is constructed (around lines 181-220 and 326-330).

**Step 2: Add registry construction**

After the existing `rooms_service` construction, add:
- Create `TrustRepoGraphReader` from the trust repo
- Create `ConstraintRegistry` from the graph reader
- Create `EngineRegistry` (empty for now — polling engine registration comes later)
- Create `EngineContext`

Don't change the existing routing yet — just construct the objects.

**Step 3: Verify tests still pass**

Run: `cargo test -p tc-service`
Expected: All existing tests PASS (no behavior change)

**Step 4: Commit**

```bash
git add service/src/main.rs
git commit -m "feat(service): wire EngineRegistry into app startup"
```

---

## Phase 4: Extract polling engine

This is the largest phase. The goal is to move poll-specific code into `tc-engine-polling` while keeping the platform room CRUD in `service/src/rooms/`.

### Task 12: Scaffold `tc-engine-polling` crate

**Files:**
- Create: `crates/tc-engine-polling/Cargo.toml`
- Create: `crates/tc-engine-polling/src/lib.rs`
- Modify: `Cargo.toml` (workspace root)
- Modify: `service/Cargo.toml` — add dependency

**Step 1: Create the crate**

```toml
# crates/tc-engine-polling/Cargo.toml
[package]
name = "tc-engine-polling"
version = "0.1.0"
edition = "2021"

[dependencies]
tc-engine-api = { path = "../tc-engine-api" }
anyhow = "1"
axum = { version = "0.7", features = ["macros"] }
chrono = { version = "0.4", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sqlx = { version = "0.8", features = ["runtime-tokio", "postgres", "uuid", "chrono"] }
tokio = { version = "1", features = ["rt", "time"] }
tracing = "0.1"
uuid = { version = "1", features = ["v4", "serde"] }
```

```rust
// crates/tc-engine-polling/src/lib.rs
pub mod engine;
// Further modules added as code is moved
```

**Step 2: Add to workspace + service dependency**

**Step 3: Verify it compiles**

Run: `cargo check -p tc-engine-polling`

**Step 4: Commit**

```bash
git add crates/tc-engine-polling/ Cargo.toml service/Cargo.toml Cargo.lock
git commit -m "chore: scaffold tc-engine-polling crate"
```

---

### Task 13: Move repo types and functions to polling engine

**Files:**
- Move: `service/src/rooms/repo/polls.rs` → `crates/tc-engine-polling/src/repo/polls.rs`
- Move: `service/src/rooms/repo/votes.rs` → `crates/tc-engine-polling/src/repo/votes.rs`
- Move: `service/src/rooms/repo/evidence.rs` → `crates/tc-engine-polling/src/repo/evidence.rs`
- Move: `service/src/rooms/repo/lifecycle_queue.rs` → `crates/tc-engine-polling/src/repo/lifecycle_queue.rs`
- Create: `crates/tc-engine-polling/src/repo/mod.rs`
- Keep: `service/src/rooms/repo/rooms.rs` (platform room CRUD)
- Modify: `service/src/rooms/repo/mod.rs` (remove moved re-exports, add re-exports from tc-engine-polling for backward compat during transition)

**Important:** This is the most delicate step. The service crate's tests import types from `crate::rooms::repo`. During the transition, keep re-exports in `service/src/rooms/repo/mod.rs` that point to the new crate locations so existing tests don't break.

**Step 1: Copy files** (not move — copy first, then update imports, then remove originals)

**Step 2: Create `crates/tc-engine-polling/src/repo/mod.rs`** with public re-exports of all moved types.

**Step 3: Add re-exports in `service/src/rooms/repo/mod.rs`**

Replace the direct module declarations with re-exports from `tc_engine_polling::repo::*`.

**Step 4: Fix all import paths** — scan for `crate::rooms::repo::` references in the service crate and ensure they still resolve.

**Step 5: Verify it compiles**

Run: `cargo check`

**Step 6: Run tests**

Run: `cargo test`
Expected: All PASS (re-exports maintain backward compat)

**Step 7: Commit**

```bash
git commit -m "refactor: move poll/vote/evidence/lifecycle repo to tc-engine-polling"
```

---

### Task 14: Split `RoomsService` trait

**Files:**
- Modify: `service/src/rooms/service.rs` — keep only room CRUD methods
- Create: `crates/tc-engine-polling/src/service.rs` — poll/vote/lifecycle methods
- Modify: `service/src/rooms/repo/mod.rs` — split `RoomsRepo` trait similarly

This is the hardest part (per the design review). The current `RoomsService` has ~20 methods. Split into:

**Platform `RoomsService` (stays in service crate):**
- `create_room`, `list_rooms`, `get_room`, `rooms_needing_content`

**Polling `PollingService` (moves to tc-engine-polling):**
- All poll, dimension, vote, lifecycle, and results methods

**Step 1: Define `PollingService` trait in tc-engine-polling** with all poll-related methods.

**Step 2: Implement `PollingService`** — move the implementations from `DefaultRoomsService` to a new `DefaultPollingService`.

**Step 3: Slim down `RoomsService`** in the service crate to room-CRUD only.

**Step 4: Update callers** — the HTTP handlers that call poll methods need to take `Arc<dyn PollingService>` instead of `Arc<dyn RoomsService>`. The room CRUD handlers keep using `Arc<dyn RoomsService>`.

**Step 5: Update `RoomsRepo` similarly** — split into platform `RoomsRepo` (room CRUD queries) and polling `PollingRepo` (poll/vote queries).

**Step 6: Verify it compiles and tests pass**

Run: `cargo check && cargo test`

**Step 7: Commit**

```bash
git commit -m "refactor: split RoomsService into platform room CRUD and polling engine service"
```

---

### Task 15: Move HTTP handlers to polling engine

**Files:**
- Create: `crates/tc-engine-polling/src/http.rs` (or `http/mod.rs`)
- Modify: `service/src/rooms/http/mod.rs` — keep only platform routes

**Step 1: Move poll-specific handlers**

Move to `tc-engine-polling`:
- `create_poll`, `list_polls`, `get_poll_detail`
- `update_poll_status`, `add_dimension`
- `cast_vote`, `get_results`, `get_distribution`, `my_votes`
- `get_agenda`, `get_capacity`
- `create_evidence`, `delete_evidence`, `reset_poll`
- All request/response types for these handlers

Keep in service crate:
- `list_rooms`, `create_room`, `get_room`
- `RoomResponse`, `CreateRoomRequest`

**Step 2: Create `PollingEngine::routes()`** that returns an Axum router with all the moved handlers.

**Step 3: Update service's room router** to only have platform routes.

**Step 4: Wire the polling engine's routes** into the app via `EngineRegistry`.

**Step 5: Verify it compiles and tests pass**

Run: `cargo check && cargo test`

**Step 6: Commit**

```bash
git commit -m "refactor: move poll HTTP handlers to tc-engine-polling"
```

---

### Task 16: Move lifecycle consumer to polling engine

**Files:**
- Move: `service/src/rooms/lifecycle.rs` logic → `crates/tc-engine-polling/src/lifecycle.rs`
- Implement: `PollingEngine::start()` to spawn the lifecycle consumer

**Step 1: Move the consumer logic**

The `spawn_lifecycle_consumer` function moves to tc-engine-polling. It now takes `EngineContext` instead of `(PgPool, Arc<dyn RoomsService>)`.

**Step 2: Implement `start()` on `PollingEngine`**

```rust
fn start(&self, ctx: EngineContext) -> Result<Vec<tokio::task::JoinHandle<()>>, anyhow::Error> {
    let handle = spawn_lifecycle_consumer(ctx.pool.clone(), /* polling service */, Duration::from_secs(5));
    Ok(vec![handle])
}
```

**Step 3: Remove lifecycle spawning from `main.rs`** — it's now handled by `engine.start()`.

**Step 4: Verify it compiles and tests pass**

**Step 5: Commit**

```bash
git commit -m "refactor: move lifecycle consumer to PollingEngine::start()"
```

---

### Task 17: Implement the full `RoomEngine` trait for `PollingEngine`

**Files:**
- Create/modify: `crates/tc-engine-polling/src/engine.rs`

**Step 1: Implement all trait methods**

```rust
impl RoomEngine for PollingEngine {
    fn engine_type(&self) -> &str { "polling" }
    fn metadata(&self) -> EngineMetadata { /* display name, description */ }
    fn routes(&self) -> axum::Router<PlatformState> { /* from Task 15 */ }
    fn config_schema(&self) -> serde_json::Value { serde_json::json!({}) }
    fn validate_config(&self, _config: &serde_json::Value) -> Result<(), EngineError> { Ok(()) }
    async fn on_room_created(&self, _room_id: Uuid, _config: &serde_json::Value, _ctx: &EngineContext) -> Result<(), EngineError> { Ok(()) }
    fn start(&self, ctx: EngineContext) -> Result<Vec<JoinHandle<()>>, anyhow::Error> { /* from Task 16 */ }
}
```

**Step 2: Register in `main.rs`**

```rust
registry.register(tc_engine_polling::engine::PollingEngine::new());
```

**Step 3: Verify it compiles and all tests pass**

Run: `cargo check && cargo test`

**Step 4: Commit**

```bash
git commit -m "feat(tc-engine-polling): implement RoomEngine trait"
```

---

### Task 18: Remove old constraint code from service crate

**Files:**
- Modify: `service/src/trust/constraints.rs` — replace with re-exports from `tc-engine-api`
- Modify: `service/src/rooms/service.rs` — use `tc_engine_api::constraints` instead of `crate::trust::constraints`

**Step 1: Replace constraint implementations** with thin re-exports:

```rust
// service/src/trust/constraints.rs
// Constraints now live in tc-engine-api. Re-export for backward compatibility.
pub use tc_engine_api::constraints::*;
```

**Step 2: Update imports** throughout the service crate.

**Step 3: Verify existing constraint integration tests still pass**

Run: `cargo test --test trust_constraint_tests`
Expected: These tests use a real DB and may need adjustment since `RoomConstraint::check` now takes `&dyn TrustGraphReader` instead of `&dyn TrustRepo`. Either update the tests to use the adapter, or keep both test suites (unit tests in tc-engine-api, integration tests in service).

**Step 4: Commit**

```bash
git commit -m "refactor: delegate constraints to tc-engine-api, remove duplicated code"
```

---

## Phase 5: Refactor frontend

### Task 19: Create `web/src/engines/api/` — the engine contract

**Files:**
- Create: `web/src/engines/api/types.ts`
- Create: `web/src/engines/api/hooks.ts`
- Create: `web/src/engines/api/index.ts`

**Step 1: Write the engine contract types**

```typescript
// web/src/engines/api/types.ts
import type { Room } from '@/features/rooms';  // temporary — will move

export interface EngineMeta {
  type: string;
  displayName: string;
  description: string;
}

export interface EngineViewProps {
  room: Room;
  roomId: string;
  eligibility: { isEligible: boolean; reason?: string };
}
```

**Step 2: Write platform-provided hooks**

```typescript
// web/src/engines/api/hooks.ts
// Re-export platform hooks that engines should use
export { useRoom } from '@/features/rooms';
export { useAuth } from '@/providers/DeviceProvider';  // adjust import path
```

**Step 3: Barrel export**

```typescript
// web/src/engines/api/index.ts
export * from './types';
export * from './hooks';
```

**Step 4: Verify lint passes**

Run: `cd web && yarn lint`

**Step 5: Commit**

```bash
git add web/src/engines/
git commit -m "feat(web): create engine API contract types and hooks"
```

---

### Task 20: Move rooms feature to `web/src/engines/polling/`

**Files:**
- Move: `web/src/features/rooms/api/` → `web/src/engines/polling/api/`
- Move: `web/src/features/rooms/components/` → `web/src/engines/polling/components/`
- Move: `web/src/features/rooms/hooks/` → `web/src/engines/polling/hooks/`
- Create: `web/src/engines/polling/PollEngineView.tsx` — extracted from `Poll.page.tsx`
- Create: `web/src/engines/polling/index.ts` — engine barrel
- Modify: `web/src/features/rooms/index.ts` — re-export from new location for backward compat

**Step 1: Copy the files to new location**

**Step 2: Update all internal import paths** within the moved files (`../api` → local paths).

**Step 3: Create backward-compat re-exports** in `web/src/features/rooms/index.ts`:

```typescript
// Temporary: re-export from engine location for backward compat
export * from '@/engines/polling/api';
export * from '@/engines/polling/hooks/usePollCountdown';
export * from '@/engines/polling/components/PollCountdown';
// ... etc
```

**Step 4: Verify lint and tests pass**

Run: `cd web && yarn lint && yarn vitest --run`

**Step 5: Commit**

```bash
git commit -m "refactor(web): move rooms feature to engines/polling/"
```

---

### Task 21: Create `EngineRouter` and registry

**Files:**
- Create: `web/src/engines/EngineRouter.tsx`
- Create: `web/src/engines/registry.ts`

**Step 1: Write the registry**

```typescript
// web/src/engines/registry.ts
export const engineMap: Record<string, () => Promise<{ EngineView: React.FC<import('./api').EngineViewProps> }>> = {
  polling: () => import('./polling'),
};
```

**Step 2: Write EngineRouter**

```typescript
// web/src/engines/EngineRouter.tsx
import React, { Suspense } from 'react';
import { Center, Loader } from '@mantine/core';
import type { EngineViewProps } from './api';
import { engineMap } from './registry';

export function EngineRouter(props: EngineViewProps) {
  const loader = engineMap[props.room.engine_type];
  if (!loader) {
    return <Center>Unknown room type: {props.room.engine_type}</Center>;
  }
  const LazyEngine = React.lazy(async () => {
    const mod = await loader();
    return { default: mod.EngineView };
  });
  return (
    <Suspense fallback={<Center><Loader /></Center>}>
      <LazyEngine {...props} />
    </Suspense>
  );
}
```

**Step 3: Create `PollEngineView.tsx`** in `web/src/engines/polling/`

Extract the main body of `Poll.page.tsx` into this component. It receives `EngineViewProps` and renders sliders, results, countdown, etc.

**Step 4: Update `Poll.page.tsx`** to be a thin shell:

```typescript
// Simplified Poll.page.tsx
import { EngineRouter } from '@/engines/EngineRouter';
import { useRoom } from '@/engines/api';
// ... fetch room, check eligibility, render EngineRouter
```

**Step 5: Create the polling engine barrel**

```typescript
// web/src/engines/polling/index.ts
export { PollEngineView as EngineView } from './PollEngineView';
export const engineMeta = {
  type: 'polling',
  displayName: 'Multi-Dimensional Poll',
  description: 'Evaluate subjects on configurable dimensions via sliders',
};
```

**Step 6: Verify lint and tests pass**

Run: `cd web && yarn lint && yarn vitest --run`

**Step 7: Commit**

```bash
git commit -m "feat(web): add EngineRouter with lazy dispatch per engine type"
```

---

## Phase 6: Verify everything works

### Task 22: Full verification pass

**Step 1: Run backend lint**

Run: `just lint-backend`
Expected: PASS

**Step 2: Run frontend lint**

Run: `just lint-frontend`
Expected: PASS

**Step 3: Run backend tests**

Run: `just test-backend`
Expected: All PASS

**Step 4: Run frontend tests**

Run: `just test-frontend`
Expected: All PASS

**Step 5: Run full lint + test suite**

Run: `just lint && just test`
Expected: All PASS

**Step 6: Run codegen to check for staleness**

Run: `just codegen`
Verify no uncommitted changes.

**Step 7: Commit any fixups, then tag the milestone**

```bash
git commit -m "chore: fix lint/test issues from engine extraction"
```

---

### Task 23: Clean up backward-compat re-exports

**Files:**
- Modify: `web/src/features/rooms/index.ts` — remove, update all consumers to import from `@/engines/`
- Modify: `service/src/rooms/repo/mod.rs` — remove re-exports of moved types if no longer needed
- Modify: `service/src/trust/constraints.rs` — remove re-export shim if no longer needed

**Step 1: Search for all imports from the old locations**

Run grep for `@/features/rooms` in `web/src/` and update to `@/engines/polling` or `@/engines/api`.

Run grep for `crate::rooms::repo::` poll/vote types in `service/` and update to `tc_engine_polling::repo::`.

**Step 2: Delete the old `web/src/features/rooms/` directory** once no consumers remain.

**Step 3: Verify everything still passes**

Run: `just lint && just test`

**Step 4: Commit**

```bash
git commit -m "chore: remove backward-compat re-exports from old room locations"
```

---

## Summary

| Phase | Tasks | What it accomplishes |
|---|---|---|
| 1 | 1–7 | `tc-engine-api` crate with all traits, types, and shared constraints |
| 2 | 8 | `TrustGraphReader` adapter in the service crate |
| 3 | 9–11 | `EngineRegistry` infrastructure wired into the app |
| 4 | 12–18 | Polling engine extracted into `tc-engine-polling` |
| 5 | 19–21 | Frontend restructured with `EngineRouter` and `engines/polling/` |
| 6 | 22–23 | Full verification and cleanup |

After this plan is complete, building a new engine (e.g. pairwise comparison) means:
1. Read `crates/tc-engine-api/` for the Rust trait
2. Read `web/src/engines/api/` for the TS interface
3. Create `crates/tc-engine-pairwise/` and `web/src/engines/pairwise/`
4. Register the engine in `main.rs` and `web/src/engines/registry.ts`
5. Never touch platform code
