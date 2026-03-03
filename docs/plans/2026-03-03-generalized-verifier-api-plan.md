# Generalized Verifier API Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the separate verifier entity type with account-based verifiers, add a general endorsement endpoint, and extract the ID.me adapter as a standalone service.

**Architecture:** Verifiers become regular accounts endorsed with `"authorized_verifier"`. A new `POST /verifiers/endorsements` endpoint lets any authorized verifier create endorsements via device-key-signed requests. The ID.me OAuth adapter is extracted into a separate binary that talks to the TC API via a Progenitor-generated Rust client.

**Tech Stack:** Rust/Axum, sqlx migrations, utoipa OpenAPI, Progenitor client codegen, tc-crypto Ed25519 signing

---

## Phasing

This plan is split into independently shippable phases:

- **Phase A (Tasks 1–11):** Core verifier API — migration, repo/service changes, config bootstrap, new endpoint, ID.me refactor to use new model (stays in-process temporarily), tests. **This is the primary deliverable for ticket #381.**
- **Phase B (Tasks 12–13):** Generated Rust API client via Progenitor.
- **Phase C (Tasks 14–15):** ID.me extraction into `tc-idme-verifier` binary, removal from TC server.
- **Phase D (Task 16):** Demo seeder migration to use API client.

---

## Phase A: Core Verifier API

### Task 1: Database migration

**Files:**
- Create: `service/migrations/10_verifier_as_account.sql`

**Step 1: Write the migration**

```sql
-- Migration: Convert verifier accounts from separate entity to regular accounts.
-- Endorsement issuer_id changes from reputation__verifier_accounts(id) to accounts(id).
-- Existing endorsements become genesis (NULL issuer) since old verifier accounts
-- have no corresponding user account.

-- 1. Drop FK from issuer_id to reputation__verifier_accounts
ALTER TABLE reputation__endorsements
    DROP CONSTRAINT reputation__endorsements_issuer_id_fkey;

-- 2. Nullify existing issuer_ids (old verifier account UUIDs have no account mapping)
UPDATE reputation__endorsements SET issuer_id = NULL;

-- 3. Allow NULL issuer (genesis endorsements)
ALTER TABLE reputation__endorsements
    ALTER COLUMN issuer_id DROP NOT NULL;

-- 4. Add FK to accounts(id) for non-NULL issuers
ALTER TABLE reputation__endorsements
    ADD CONSTRAINT fk_endorsements_issuer
    FOREIGN KEY (issuer_id) REFERENCES accounts(id);

-- 5. Replace old unique constraint with wider one
ALTER TABLE reputation__endorsements
    DROP CONSTRAINT uq_endorsements_subject_topic;

-- Multiple verifiers can endorse the same (subject, topic)
CREATE UNIQUE INDEX uq_endorsements_subject_topic_issuer
    ON reputation__endorsements (subject_id, topic, issuer_id);

-- Prevent duplicate genesis endorsements (PostgreSQL treats NULLs as distinct in UNIQUE)
CREATE UNIQUE INDEX uq_endorsements_genesis
    ON reputation__endorsements (subject_id, topic) WHERE issuer_id IS NULL;

-- 6. Drop the old verifier accounts table (no longer needed)
DROP TABLE reputation__verifier_accounts;
```

**Step 2: Verify migration compiles with existing tests**

Run: `cd service && cargo test migration -- --ignored 2>&1 | head -20`

The migration test runner should pick up the new file. Existing data tests will break until repo layer is updated — that's expected.

**Step 3: Commit**

```bash
git add service/migrations/10_verifier_as_account.sql
git commit -m "feat(migration): convert verifiers from separate entity to accounts (#381)"
```

---

### Task 2: Update endorsement repo types for nullable issuer_id

**Files:**
- Modify: `service/src/reputation/repo/endorsements.rs`

**Step 1: Change `issuer_id` to `Option<Uuid>` in record types and SQL row**

In `EndorsementRecord` (line 13), `EndorsementRow` (line 45), and `row_to_record` — change `issuer_id: Uuid` to `issuer_id: Option<Uuid>`.

**Step 2: Update `create_endorsement` to accept `Option<Uuid>`**

Change the function signature (line 73) from `issuer_id: Uuid` to `issuer_id: Option<Uuid>`. The SQL INSERT already binds `$4` which sqlx handles for `Option<Uuid>`.

Update the duplicate constraint name check (line 104) from `"uq_endorsements_subject_topic"` to `"uq_endorsements_subject_topic_issuer"`. Also add a check for `"uq_endorsements_genesis"`.

**Step 3: Update `get_endorsement_by_subject_and_topic` to filter non-revoked**

This function (line 170) should remain unchanged — it returns endorsements regardless of issuer. Multiple rows are now possible, but the existing callers don't use this function via the trait.

**Step 4: Verify the repo module compiles**

Run: `cd service && cargo check 2>&1 | head -30`

Expected: compilation errors in `repo/mod.rs` and `service.rs` (they still reference old signature). Fix in next tasks.

**Step 5: Commit**

```bash
git add service/src/reputation/repo/endorsements.rs
git commit -m "refactor(repo): make endorsement issuer_id nullable (#381)"
```

---

### Task 3: Remove verifier_accounts module and update ReputationRepo trait

**Files:**
- Delete: `service/src/reputation/repo/verifier_accounts.rs`
- Modify: `service/src/reputation/repo/mod.rs`

**Step 1: Delete `verifier_accounts.rs`**

Remove the file entirely.

**Step 2: Update `mod.rs` — remove verifier account exports and trait methods**

Remove:
- `pub mod verifier_accounts;` (line 5)
- The `pub use verifier_accounts::...` block (lines 15-18)
- The `ensure_verifier_account` and `get_verifier_account_by_name` methods from the `ReputationRepo` trait (lines 50-59)
- The corresponding `PgReputationRepo` implementations (lines 116-129)

Update the `create_endorsement` signature in the trait (line 33) and impl (line 95) to use `Option<Uuid>` for `issuer_id`.

**Step 3: Verify the repo module compiles in isolation**

Run: `cd service && cargo check 2>&1 | head -30`

Expected: errors in `service.rs` and `idme.rs` which reference `VerifierAccountRepoError` and `get_verifier_account_by_name`. Fixed in next tasks.

**Step 4: Commit**

```bash
git add -A service/src/reputation/repo/
git commit -m "refactor(repo): remove verifier_accounts, widen endorsement trait (#381)"
```

---

### Task 4: Update EndorsementService for account-based verifiers

**Files:**
- Modify: `service/src/reputation/service.rs`

**Step 1: Simplify `create_endorsement` — remove verifier name lookup**

The service method currently takes `verifier_name: &str` and looks up the verifier by name. Replace with `issuer_id: Option<Uuid>` — the caller provides the issuer directly (either their own account_id or None for genesis).

Remove:
- The `VerifierNotFound` error variant (line 25)
- The verifier name lookup logic (lines 86-102)
- The `bootstrap_idme_verifier` function (lines 161-169)
- The import of `VerifierAccountRepoError` (line 13)

Updated trait:

```rust
async fn create_endorsement(
    &self,
    subject_id: Uuid,
    topic: &str,
    issuer_id: Option<Uuid>,
    evidence: Option<&serde_json::Value>,
) -> Result<CreatedEndorsement, EndorsementError>;
```

Updated impl — just validates topic and delegates to repo:

```rust
async fn create_endorsement(
    &self,
    subject_id: Uuid,
    topic: &str,
    issuer_id: Option<Uuid>,
    evidence: Option<&serde_json::Value>,
) -> Result<CreatedEndorsement, EndorsementError> {
    if topic.is_empty() {
        return Err(EndorsementError::Validation(
            "Topic cannot be empty".to_string(),
        ));
    }

    self.repo
        .create_endorsement(subject_id, topic, issuer_id, evidence)
        .await
        .map_err(|e| match e {
            EndorsementRepoError::Duplicate => EndorsementError::Duplicate,
            EndorsementRepoError::NotFound => {
                tracing::error!("Unexpected NotFound during endorsement creation");
                EndorsementError::Internal("Internal server error".to_string())
            }
            EndorsementRepoError::Database(e) => {
                tracing::error!("Endorsement creation failed: {e}");
                EndorsementError::Internal("Internal server error".to_string())
            }
        })
}
```

**Step 2: Remove `VerifierNotFound` from HTTP error mapping**

In `service/src/reputation/http/mod.rs` (line 140-146), remove the `EndorsementError::VerifierNotFound` match arm from `endorsement_error_response`.

**Step 3: Verify compilation**

Run: `cd service && cargo check 2>&1 | head -30`

Expected: errors in `idme.rs` which calls `create_endorsement` with old signature. Fixed in Task 5.

**Step 4: Commit**

```bash
git add service/src/reputation/service.rs service/src/reputation/http/mod.rs
git commit -m "refactor(service): simplify EndorsementService to accept issuer_id directly (#381)"
```

---

### Task 5: Refactor ID.me callback to use account-based model (temporary in-process)

**Files:**
- Modify: `service/src/reputation/http/idme.rs`
- Modify: `service/src/main.rs`
- Modify: `service/src/config.rs`

The ID.me adapter stays in-process for now but is refactored to use the new service layer. It will be extracted in Phase C.

**Step 1: Add verifier account_id to ID.me config or runtime state**

Add a new Extension that carries the bootstrapped verifier's account_id. In `main.rs`, after config bootstrap creates the verifier account, store its `account_id` for the callback to use.

Add a simple wrapper type in `idme.rs`:

```rust
#[derive(Clone)]
pub struct IdMeVerifierAccountId(pub Uuid);
```

**Step 2: Update `create_verification_endorsement` to use `issuer_id`**

In `idme.rs`, change `create_verification_endorsement` (line 293) to pass `Some(verifier_account_id)` instead of the verifier name:

```rust
async fn create_verification_endorsement(
    service: &dyn EndorsementService,
    account_id: Uuid,
    verifier_account_id: Uuid,
    idme_sub: &str,
) -> Result<(), String> {
    match service
        .create_endorsement(account_id, "identity_verified", Some(verifier_account_id), None)
        .await
    { ... }
}
```

Update `process_callback` to extract the verifier account_id from the Extension and pass it through.

**Step 3: Update main.rs to bootstrap verifier as an account**

This is a temporary bridge — the config bootstrap (Task 7) will replace it. For now, keep using the ID.me config to detect whether to bootstrap, but create a real account instead of a verifier_account row.

Use `IdentityRepo::create_signup` or direct SQL to ensure an "idme" account exists, then create the `authorized_verifier` endorsement with `issuer_id = None`.

Pass `IdMeVerifierAccountId` as an Extension.

**Step 4: Verify compilation and existing tests pass**

Run: `cd service && cargo test 2>&1 | tail -20`

**Step 5: Commit**

```bash
git add service/src/reputation/http/idme.rs service/src/main.rs service/src/config.rs
git commit -m "refactor(idme): use account-based verifier model (#381)"
```

---

### Task 6: Config-driven verifier bootstrap

**Files:**
- Modify: `service/src/config.rs`
- Create: `service/src/reputation/bootstrap.rs`
- Modify: `service/src/reputation/mod.rs`
- Modify: `service/src/main.rs`

**Step 1: Add VerifierConfig to config.rs**

```rust
/// Configuration for a platform-bootstrapped verifier account.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VerifierConfig {
    /// Username for the verifier account.
    pub name: String,
    /// Base64url-encoded Ed25519 public key (root key for the verifier account).
    pub public_key: String,
}
```

Add to `Config`:

```rust
/// Platform verifiers bootstrapped at startup. Each entry creates an account
/// (if missing) and grants the `authorized_verifier` endorsement.
/// Set via `TC_VERIFIERS` as a JSON array.
#[serde(default)]
pub verifiers: Vec<VerifierConfig>,
```

**Step 2: Write the bootstrap module**

`service/src/reputation/bootstrap.rs`:

```rust
//! Config-driven verifier bootstrap.
//!
//! At startup, ensures each configured verifier has:
//! 1. A user account (created if missing)
//! 2. An `authorized_verifier` endorsement with NULL issuer (genesis)

use sqlx::PgPool;
use uuid::Uuid;

use crate::config::VerifierConfig;

pub struct BootstrappedVerifier {
    pub name: String,
    pub account_id: Uuid,
}

/// Bootstrap all configured verifiers. Idempotent — safe to call on every startup.
pub async fn bootstrap_verifiers(
    pool: &PgPool,
    verifiers: &[VerifierConfig],
) -> Result<Vec<BootstrappedVerifier>, anyhow::Error> {
    let mut result = Vec::with_capacity(verifiers.len());

    for v in verifiers {
        let account_id = ensure_verifier_account(pool, &v.name, &v.public_key).await?;
        ensure_authorized_verifier_endorsement(pool, account_id).await?;
        tracing::info!(name = %v.name, account_id = %account_id, "Verifier bootstrapped");
        result.push(BootstrappedVerifier {
            name: v.name.clone(),
            account_id,
        });
    }

    Ok(result)
}

/// Ensure an account exists for this verifier. Returns account_id.
async fn ensure_verifier_account(
    pool: &PgPool,
    name: &str,
    public_key: &str,
) -> Result<Uuid, anyhow::Error> {
    // Decode and derive KID from public key
    let pubkey_bytes = tc_crypto::decode_base64url(public_key)
        .map_err(|e| anyhow::anyhow!("Invalid verifier public key for {name}: {e}"))?;
    let kid = tc_crypto::Kid::derive(&pubkey_bytes);

    // Try to find existing account by root_kid
    let existing = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM accounts WHERE root_kid = $1"
    )
    .bind(kid.as_str())
    .fetch_optional(pool)
    .await?;

    if let Some(id) = existing {
        return Ok(id);
    }

    // Create new account
    let id = Uuid::new_v4();
    sqlx::query(
        r"INSERT INTO accounts (id, username, root_pubkey, root_kid)
          VALUES ($1, $2, $3, $4)
          ON CONFLICT (username) DO UPDATE SET username = EXCLUDED.username
          RETURNING id"
    )
    .bind(id)
    .bind(name)
    .bind(public_key)
    .bind(kid.as_str())
    .execute(pool)
    .await?;

    Ok(id)
}

/// Ensure the account has an authorized_verifier endorsement (genesis, NULL issuer).
async fn ensure_authorized_verifier_endorsement(
    pool: &PgPool,
    account_id: Uuid,
) -> Result<(), anyhow::Error> {
    sqlx::query(
        r"INSERT INTO reputation__endorsements (id, subject_id, topic, issuer_id)
          VALUES (gen_random_uuid(), $1, 'authorized_verifier', NULL)
          ON CONFLICT (subject_id, topic) WHERE issuer_id IS NULL DO NOTHING"
    )
    .bind(account_id)
    .execute(pool)
    .await?;

    Ok(())
}
```

**Step 3: Register module and wire into main.rs**

Add `pub mod bootstrap;` to `service/src/reputation/mod.rs`.

In `main.rs`, replace the `bootstrap_idme_verifier` call (lines 140-144) with:

```rust
let bootstrapped_verifiers = reputation::bootstrap::bootstrap_verifiers(&pool, &config.verifiers)
    .await
    .map_err(|e| anyhow::anyhow!("Failed to bootstrap verifiers: {e}"))?;
```

If ID.me config is present, find the "idme" verifier in the bootstrapped list and set up its `IdMeVerifierAccountId` extension.

**Step 4: Run tests**

Run: `cd service && cargo test 2>&1 | tail -20`

**Step 5: Commit**

```bash
git add service/src/reputation/bootstrap.rs service/src/reputation/mod.rs \
    service/src/config.rs service/src/main.rs
git commit -m "feat(bootstrap): config-driven verifier account bootstrap (#381)"
```

---

### Task 7: Write failing test for POST /verifiers/endorsements

**Files:**
- Create or modify: `service/tests/endorsement_api_tests.rs`

**Step 1: Write integration test for the happy path**

```rust
//! Integration tests for POST /verifiers/endorsements endpoint.

mod common;

use axum::{body::Body, http::{header::CONTENT_TYPE, Method, Request, StatusCode}};
use serde_json::{json, Value};
use tower::ServiceExt;

use common::app_builder::TestAppBuilder;
use common::factories::{build_authed_request, valid_signup_with_keys};
use common::test_db::isolated_db;
use tc_test_macros::shared_runtime_test;
use tinycongress_api::reputation::repo::has_endorsement;

/// Helper: sign up a user and return (keys, account_id).
async fn signup_user(
    app: &axum::Router,
    username: &str,
) -> (common::factories::SignupKeys, uuid::Uuid) {
    let (json, keys) = valid_signup_with_keys(username);
    let response = app.clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(json))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.expect("body");
    let json: Value = serde_json::from_slice(&body).expect("json");
    let account_id = json["account_id"].as_str().expect("account_id").parse().expect("uuid");
    (keys, account_id)
}

#[shared_runtime_test]
async fn test_verifier_can_create_endorsement() {
    let pool = isolated_db().await;
    let app = TestAppBuilder::new().with_rooms_pool(pool.clone()).build();

    // Sign up a verifier account and a target user
    let (verifier_keys, verifier_id) = signup_user(&app, "test-verifier").await;
    let (_user_keys, user_id) = signup_user(&app, "target-user").await;

    // Bootstrap verifier endorsement (genesis)
    tinycongress_api::reputation::repo::create_endorsement(
        &pool, verifier_id, "authorized_verifier", None, None,
    ).await.expect("bootstrap");

    // Call POST /verifiers/endorsements
    let body = json!({
        "username": "target-user",
        "topic": "identity_verified"
    });

    let request = build_authed_request(
        &verifier_keys,
        Method::POST,
        "/verifiers/endorsements",
        body.to_string(),
    );

    let response = app.clone().oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::CREATED);

    // Verify endorsement was created
    let has = has_endorsement(&pool, user_id, "identity_verified")
        .await
        .expect("check");
    assert!(has);
}

#[shared_runtime_test]
async fn test_non_verifier_gets_403() {
    let pool = isolated_db().await;
    let app = TestAppBuilder::new().with_rooms_pool(pool.clone()).build();

    let (keys, _) = signup_user(&app, "regular-user").await;
    let _ = signup_user(&app, "target-user").await;

    let body = json!({
        "username": "target-user",
        "topic": "identity_verified"
    });

    let request = build_authed_request(
        &keys,
        Method::POST,
        "/verifiers/endorsements",
        body.to_string(),
    );

    let response = app.clone().oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[shared_runtime_test]
async fn test_endorsement_unknown_user_returns_404() {
    let pool = isolated_db().await;
    let app = TestAppBuilder::new().with_rooms_pool(pool.clone()).build();

    let (verifier_keys, verifier_id) = signup_user(&app, "test-verifier").await;

    // Bootstrap verifier
    tinycongress_api::reputation::repo::create_endorsement(
        &pool, verifier_id, "authorized_verifier", None, None,
    ).await.expect("bootstrap");

    let body = json!({
        "username": "nonexistent-user",
        "topic": "identity_verified"
    });

    let request = build_authed_request(
        &verifier_keys,
        Method::POST,
        "/verifiers/endorsements",
        body.to_string(),
    );

    let response = app.clone().oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[shared_runtime_test]
async fn test_duplicate_endorsement_returns_409() {
    let pool = isolated_db().await;
    let app = TestAppBuilder::new().with_rooms_pool(pool.clone()).build();

    let (verifier_keys, verifier_id) = signup_user(&app, "test-verifier").await;
    let _ = signup_user(&app, "target-user").await;

    tinycongress_api::reputation::repo::create_endorsement(
        &pool, verifier_id, "authorized_verifier", None, None,
    ).await.expect("bootstrap");

    let body = json!({
        "username": "target-user",
        "topic": "identity_verified"
    });

    // First call — should succeed
    let request = build_authed_request(
        &verifier_keys,
        Method::POST,
        "/verifiers/endorsements",
        body.to_string(),
    );
    let response = app.clone().oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::CREATED);

    // Second call — same verifier, same subject+topic → 409
    let request = build_authed_request(
        &verifier_keys,
        Method::POST,
        "/verifiers/endorsements",
        body.to_string(),
    );
    let response = app.clone().oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::CONFLICT);
}
```

**Step 2: Run tests to verify they fail**

Run: `cd service && cargo test endorsement_api -- 2>&1 | tail -20`

Expected: compilation errors (endpoint doesn't exist yet).

**Step 3: Commit**

```bash
git add service/tests/endorsement_api_tests.rs
git commit -m "test: add failing tests for POST /verifiers/endorsements (#381)"
```

---

### Task 8: Implement POST /verifiers/endorsements endpoint

**Files:**
- Modify: `service/src/reputation/http/mod.rs`

**Step 1: Add request/response types**

```rust
#[derive(Debug, Deserialize)]
pub struct CreateEndorsementRequest {
    pub username: String,
    pub topic: String,
    #[serde(default)]
    pub evidence: Option<serde_json::Value>,
    #[serde(default)]
    pub external_identity: Option<ExternalIdentityRequest>,
}

#[derive(Debug, Deserialize)]
pub struct ExternalIdentityRequest {
    pub provider: String,
    pub provider_subject: String,
}

#[derive(Debug, Serialize)]
pub struct CreatedEndorsementResponse {
    pub id: Uuid,
    pub subject_id: Uuid,
    pub topic: String,
    pub issuer_id: Uuid,
    pub created_at: String,
}
```

**Step 2: Add the handler**

```rust
use crate::identity::repo::IdentityRepo;

/// Create an endorsement as an authorized verifier.
async fn create_endorsement_as_verifier(
    Extension(endorsement_service): Extension<Arc<dyn EndorsementService>>,
    Extension(reputation_repo): Extension<Arc<dyn ReputationRepo>>,
    Extension(identity_repo): Extension<Arc<dyn IdentityRepo>>,
    auth: AuthenticatedDevice,
    Json(body): Json<CreateEndorsementRequest>,
) -> impl IntoResponse {
    // 1. Check caller is an authorized verifier
    let is_verifier = match endorsement_service
        .has_endorsement(auth.account_id, "authorized_verifier")
        .await
    {
        Ok(has) => has,
        Err(e) => return endorsement_error_response(e),
    };
    if !is_verifier {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "Account is not an authorized verifier".to_string(),
            }),
        )
            .into_response();
    }

    // 2. Resolve username → account_id
    let subject = match identity_repo.get_account_by_username(&body.username).await {
        Ok(account) => account,
        Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "User not found".to_string(),
                }),
            )
                .into_response();
        }
    };

    // 3. Optional sybil check via external identity
    if let Some(ref ext_id) = body.external_identity {
        if let Err(e) = link_identity_if_new(
            &*reputation_repo,
            subject.id,
            &ext_id.provider,
            &ext_id.provider_subject,
        )
        .await
        {
            return (
                StatusCode::CONFLICT,
                Json(ErrorResponse { error: e }),
            )
                .into_response();
        }
    }

    // 4. Create endorsement
    match endorsement_service
        .create_endorsement(
            subject.id,
            &body.topic,
            Some(auth.account_id),
            body.evidence.as_ref(),
        )
        .await
    {
        Ok(created) => (
            StatusCode::CREATED,
            Json(CreatedEndorsementResponse {
                id: created.id,
                subject_id: created.subject_id,
                topic: created.topic,
                issuer_id: auth.account_id,
                created_at: chrono::Utc::now().to_rfc3339(),
            }),
        )
            .into_response(),
        Err(e) => endorsement_error_response(e),
    }
}
```

**Step 3: Extract sybil check helper from idme.rs to mod.rs**

Move the `link_identity_if_new` logic from `idme.rs` into a shared function in `mod.rs` (or a new `sybil.rs` submodule) so both the ID.me callback and the new endpoint can call it.

```rust
/// Check if an external identity is already linked to a different account.
/// If new, link it. If same account, no-op. If different account, reject.
pub(crate) async fn link_identity_if_new(
    repo: &dyn ReputationRepo,
    account_id: Uuid,
    provider: &str,
    provider_subject: &str,
) -> Result<(), String> {
    match repo.get_external_identity_by_provider(provider, provider_subject).await {
        Ok(existing) => {
            if existing.account_id != account_id {
                tracing::warn!(
                    provider = %provider,
                    provider_subject = %provider_subject,
                    existing_account = %existing.account_id,
                    requesting_account = %account_id,
                    "Sybil attempt: external identity already linked to different account"
                );
                return Err("This identity is already linked to another account".to_string());
            }
            Ok(()) // Same account re-verifying
        }
        Err(crate::reputation::repo::ExternalIdentityRepoError::NotFound) => repo
            .link_external_identity(account_id, provider, provider_subject)
            .await
            .map(|_| ())
            .map_err(|e| {
                tracing::error!("Failed to link external identity: {e}");
                "Verification failed".to_string()
            }),
        Err(e) => {
            tracing::error!("External identity lookup failed: {e}");
            Err("Verification failed".to_string())
        }
    }
}
```

**Step 4: Add route to router**

Update the `router()` function to include:

```rust
.route("/verifiers/endorsements", axum::routing::post(create_endorsement_as_verifier))
```

**Step 5: Run tests**

Run: `cd service && cargo test endorsement_api -- 2>&1 | tail -30`

Expected: all 4 tests pass.

**Step 6: Commit**

```bash
git add service/src/reputation/http/mod.rs service/src/reputation/http/idme.rs
git commit -m "feat(api): add POST /verifiers/endorsements endpoint (#381)"
```

---

### Task 9: Update existing test helpers for new model

**Files:**
- Modify: `service/tests/rooms_handler_tests.rs` (the `endorse_user` helper)
- Any other test files using `ensure_verifier_account`

**Step 1: Update `endorse_user` helper**

The current helper (line 58-68) calls `ensure_verifier_account` which no longer exists. Replace with direct endorsement creation using `None` issuer:

```rust
async fn endorse_user(pool: &sqlx::PgPool, account_id: uuid::Uuid, topic: &str) {
    use tinycongress_api::reputation::repo::create_endorsement;
    create_endorsement(pool, account_id, topic, None, None)
        .await
        .expect("endorsement");
}
```

**Step 2: Search for other callers**

Run: `grep -r "ensure_verifier_account" service/tests/`

Update any other test files that reference the old function.

**Step 3: Run full test suite**

Run: `cd service && cargo test 2>&1 | tail -30`

Expected: all tests pass.

**Step 4: Commit**

```bash
git add service/tests/
git commit -m "test: update test helpers for account-based verifier model (#381)"
```

---

### Task 10: Add OpenAPI annotations to new endpoint

**Files:**
- Modify: `service/src/reputation/http/mod.rs`
- Modify: `service/src/rest.rs`

**Step 1: Add utoipa annotations to request/response types**

Add `ToSchema` derive to `CreateEndorsementRequest`, `ExternalIdentityRequest`, and `CreatedEndorsementResponse`.

**Step 2: Add `#[utoipa::path(...)]` to the handler**

```rust
#[utoipa::path(
    post,
    path = "/verifiers/endorsements",
    tag = "Verifiers",
    request_body = CreateEndorsementRequest,
    responses(
        (status = 201, description = "Endorsement created", body = CreatedEndorsementResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Not an authorized verifier"),
        (status = 404, description = "User not found"),
        (status = 409, description = "Duplicate endorsement or identity conflict"),
        (status = 500, description = "Internal server error")
    )
)]
```

**Step 3: Register in ApiDoc**

In `service/src/rest.rs`, add the path and schemas to the `#[openapi(...)]` attribute on `ApiDoc`.

**Step 4: Regenerate OpenAPI spec**

Run: `cd service && cargo run --bin export_openapi > ../web/openapi.json`

**Step 5: Run snapshot tests**

Run: `cd service && cargo test openapi_snapshot 2>&1 | tail -10`

If snapshots need updating, update them.

**Step 6: Commit**

```bash
git add service/src/reputation/http/mod.rs service/src/rest.rs web/openapi.json \
    service/tests/
git commit -m "feat(openapi): annotate POST /verifiers/endorsements (#381)"
```

---

### Task 11: Run full test suite and lint

**Step 1: Run linting**

Run: `just lint`

**Step 2: Run full tests**

Run: `just test`

**Step 3: Fix any issues found**

**Step 4: Commit any fixes**

```bash
git commit -m "fix: address lint and test issues (#381)"
```

---

## Phase B: Generated Rust API Client

### Task 12: Set up tc-api-client crate with Progenitor

**Files:**
- Create: `crates/tc-api-client/Cargo.toml`
- Create: `crates/tc-api-client/src/lib.rs`
- Create: `crates/tc-api-client/build.rs`
- Modify: `Cargo.toml` (workspace members)

**Step 1: Add progenitor dependency and crate structure**

```toml
# crates/tc-api-client/Cargo.toml
[package]
name = "tc-api-client"
version = "0.1.0"
edition = "2021"

[dependencies]
progenitor-client = "0.9"
reqwest = { version = "0.12", features = ["json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["serde"] }
chrono = { version = "0.4", features = ["serde"] }

[build-dependencies]
progenitor = "0.9"
```

**Step 2: Write build.rs for codegen**

```rust
fn main() {
    let spec = std::fs::read_to_string("../../web/openapi.json")
        .expect("Failed to read OpenAPI spec");
    let mut out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    out.push("codegen.rs");

    let content = progenitor::Generator::default()
        .generate_text(&serde_json::from_str(&spec).unwrap())
        .expect("Failed to generate client");

    std::fs::write(out, content).expect("Failed to write codegen");
    println!("cargo:rerun-if-changed=../../web/openapi.json");
}
```

**Step 3: Write lib.rs**

```rust
include!(concat!(env!("OUT_DIR"), "/codegen.rs"));
```

**Step 4: Add to workspace**

Add `"crates/tc-api-client"` to workspace members in root `Cargo.toml`.

**Step 5: Verify it builds**

Run: `cargo build -p tc-api-client 2>&1 | tail -10`

**Step 6: Commit**

```bash
git add crates/tc-api-client/ Cargo.toml Cargo.lock
git commit -m "feat: add tc-api-client crate with Progenitor codegen (#381)"
```

---

### Task 13: Add device key signing layer to tc-api-client

**Files:**
- Modify: `crates/tc-api-client/Cargo.toml` (add `tc-crypto` dep)
- Create: `crates/tc-api-client/src/signing.rs`

**Step 1: Add a signing middleware/wrapper**

Create a wrapper around the generated client that intercepts requests and adds the `X-Device-Kid`, `X-Signature`, `X-Timestamp`, `X-Nonce` headers using `tc-crypto` for Ed25519 signing.

This mirrors the canonical message format from `AuthenticatedDevice` (see `service/src/identity/http/auth.rs` line 175):

```
METHOD\nPATH_AND_QUERY\nTIMESTAMP\nNONCE\nBODY_SHA256_HEX
```

**Step 2: Write a test that signs and verifies a request round-trip**

**Step 3: Commit**

```bash
git add crates/tc-api-client/
git commit -m "feat(api-client): add device key request signing (#381)"
```

---

## Phase C: ID.me Verifier Extraction

### Task 14: Create tc-idme-verifier binary

**Files:**
- Create: `crates/tc-idme-verifier/Cargo.toml`
- Create: `crates/tc-idme-verifier/src/main.rs`
- Modify: `Cargo.toml` (workspace members)

The standalone binary:
- Depends on `tc-api-client` and `tc-crypto`
- Serves `GET /authorize` (requires device-key-signed request from user)
- Serves `GET /callback` (unauthenticated, from ID.me redirect)
- Validates user identity by forwarding their auth to `GET /me` on TC API
- Creates endorsements via `POST /verifiers/endorsements` on TC API
- Configuration: ID.me OAuth credentials, TC API base URL, own device key path, own HMAC state secret

The OAuth flow logic (state signing, code exchange, userinfo fetch) moves from `service/src/reputation/http/idme.rs` into this binary.

**Step 1: Scaffold the binary**

**Step 2: Move OAuth logic from idme.rs**

**Step 3: Implement the auth forwarding flow**

**Step 4: Integration test with mock TC API**

**Step 5: Commit**

---

### Task 15: Remove ID.me from TC server

**Files:**
- Delete: `service/src/reputation/http/idme.rs`
- Modify: `service/src/reputation/http/mod.rs` (remove `pub mod idme` and routes)
- Modify: `service/src/config.rs` (remove `IdMeConfig`)
- Modify: `service/src/main.rs` (remove ID.me-specific wiring)

**Step 1: Remove idme module and routes**

**Step 2: Remove IdMeConfig from config.rs**

**Step 3: Clean up main.rs — remove idme extension and bootstrap bridge**

**Step 4: Run full test suite**

Run: `just test`

**Step 5: Commit**

```bash
git add -A service/src/ service/tests/
git commit -m "refactor: extract ID.me adapter from TC server (#381)"
```

---

## Phase D: Demo Seeder Migration

### Task 16: Update demo seeder to use API client

**Files:**
- Modify: whatever seeder files exist (e.g., PR #380's seed module)

**Step 1: Replace direct DB endorsement calls with `tc-api-client` HTTP calls**

The seeder authenticates as a verifier account (using its device key) and calls `POST /verifiers/endorsements` for each user it needs to endorse.

**Step 2: Test seeder against running TC server**

**Step 3: Commit**

```bash
git commit -m "refactor(seed): use verifier API instead of direct DB access (#381)"
```

---

## Verification Checklist

After all tasks complete:

- [ ] `just lint` passes
- [ ] `just test` passes
- [ ] `reputation__verifier_accounts` table no longer exists
- [ ] `reputation__endorsements.issuer_id` references `accounts(id)`, nullable
- [ ] `POST /verifiers/endorsements` works with device key auth
- [ ] Non-verifier accounts get 403
- [ ] External identity sybil check works when provided
- [ ] OpenAPI spec is updated and snapshot tests pass
- [ ] ID.me verification still works end-to-end (in-process in Phase A, extracted in Phase C)
- [ ] No hardcoded verifier names in TC server code (outside of tests)
