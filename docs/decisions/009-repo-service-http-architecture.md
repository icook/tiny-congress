# ADR-009: Repo/Service/HTTP Three-Layer Architecture

## Status
Accepted

## Context

The identity module handles cryptographic signup, device key management, and backup operations. These operations involve input validation (base64url decoding, byte-length enforcement, Ed25519 signature verification), business rules (device limits, uniqueness), and database persistence — concerns that should not live in one place.

Several tensions shaped this decision:

- **Testability vs. simplicity.** A single handler function is easy to write but impossible to test in isolation — you can't test validation without a database, or HTTP status mapping without real crypto. Separating concerns lets each layer have focused tests with appropriate mocks.
- **Transaction sharing.** Atomic signup requires three inserts in one PostgreSQL transaction. The persistence layer needs to accept a transaction handle from the caller, not own its own connection.
- **Making wrong code hard to write.** Once data passes validation, downstream code should not need to re-validate. The type system should enforce this — not comments or conventions.

## Decision

### Three layers

The identity module is organized into three layers with strict dependency direction:

```
HTTP handler (thin adapter)
    ↓ calls
IdentityService (validation + orchestration)
    ↓ calls
IdentityRepo (persistence)
```

### IdentityRepo: consolidated trait with 11 methods

A single `IdentityRepo` trait (in `service/src/identity/repo/identity.rs`) defines all persistence operations:

**Account operations (1):**
- `create_account` — insert account row with username, root pubkey, and root KID

**Backup operations (3):**
- `create_backup` — store encrypted backup envelope
- `get_backup_by_kid` — retrieve backup by root KID
- `delete_backup_by_kid` — remove backup

**Device key operations (6):**
- `create_device_key` — insert device key with certificate (enforces 10-device limit via `SELECT ... FOR UPDATE`)
- `list_device_keys_by_account` — list all device keys for an account
- `get_device_key_by_kid` — retrieve single device key
- `revoke_device_key` — set `revoked_at` timestamp
- `rename_device_key` — update device name
- `touch_device_key` — update `last_used_at` timestamp

**Compound operation (1):**
- `create_signup` — atomic transaction wrapping account creation, backup storage, and first device key registration. Accepts `&ValidatedSignup` and rolls back entirely on any failure.

### Executor generics for transaction sharing

Module-level `create_*_with_executor` functions accept a generic `sqlx::Executor` (or `&mut PgConnection` for operations requiring `FOR UPDATE` locks). This lets `create_signup` pass its transaction handle to each sub-operation:

```rust
pub async fn create_account_with_executor<'e, E>(
    executor: E, username: &str, root_pubkey: &str, root_kid: &Kid,
) -> Result<CreatedAccount, AccountRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>
```

`create_device_key_with_executor` takes `&mut PgConnection` instead of a generic executor because it issues `SELECT ... FOR UPDATE` to serialize concurrent device additions.

### ValidatedSignup with `pub(crate)` fields

`ValidatedSignup` is the boundary type between service and repo:

```rust
pub struct ValidatedSignup {
    pub(crate) username: String,
    pub(crate) root_pubkey: String,
    pub(crate) root_kid: Kid,
    pub(crate) backup_bytes: Vec<u8>,
    pub(crate) backup_salt: Vec<u8>,
    pub(crate) backup_version: i32,
    pub(crate) device_pubkey: String,
    pub(crate) device_kid: Kid,
    pub(crate) device_name: String,
    pub(crate) certificate: Vec<u8>,
}
```

All fields are `pub(crate)` — only code inside the crate can construct it. In production, only `DefaultIdentityService::signup` does so after full validation. Test code has a separate `#[cfg(any(test, feature = "test-utils"))]` constructor.

### IdentityService: single `signup` method

The `IdentityService` trait currently exposes one method:

```rust
#[async_trait]
pub trait IdentityService: Send + Sync {
    async fn signup(&self, req: &SignupRequest) -> Result<SignupResult, SignupError>;
}
```

`DefaultIdentityService` owns an `Arc<dyn IdentityRepo>` and performs all validation before constructing `ValidatedSignup`: username rules, base64url decoding, byte-length enforcement (`[u8; 32]` for pubkeys, `[u8; 64]` for signatures), `BackupEnvelope::parse()`, and Ed25519 certificate verification.

`SignupError` maps repo errors to domain errors (e.g., `AccountRepoError::DuplicateUsername` becomes `SignupError::DuplicateUsername`). Database details are logged server-side but never exposed to callers.

### HTTP handler: thin adapter

The handler in `service/src/identity/http/mod.rs` is ~15 lines of logic:

1. Deserialize `Json<SignupRequest>`
2. Call `service.signup(&req).await`
3. Map `Ok` to `201 CREATED` with `SignupResponse`
4. Map each `SignupError` variant to an HTTP status code:
   - `Validation` → 400
   - `DuplicateUsername` / `DuplicateKey` → 409
   - `MaxDevicesReached` → 422
   - `Internal` → 500 (safe generic message; credentials scrubbed)

### Mock strategy

Two mock types enable focused testing at each boundary:

- **`MockIdentityRepo`** — used with `DefaultIdentityService` to test validation logic without a database. Configurable via `set_signup_result()` (single-use, `Mutex<Option<_>>`). Other methods return stubbed defaults.
- **`MockIdentityService`** — used with the HTTP handler to test status code mapping without running validation or crypto. `succeeding()` constructor pre-configures a happy-path result.

Both are gated behind `#[cfg(any(test, feature = "test-utils"))]`.

## Consequences

### Positive
- Validation tests run without a database (~ms per test). HTTP tests run without crypto operations.
- `ValidatedSignup`'s `pub(crate)` fields make it structurally impossible to skip validation outside of tests.
- Executor generics let `create_signup` share a transaction across sub-operations without leaking transaction management into the service layer.
- Each layer has a focused error type. Database constraint names are encapsulated in the repo; HTTP status codes are encapsulated in the handler.
- Adding a new identity operation (e.g., device key rotation) follows a clear pattern: add a repo method, add a service method with validation, add a thin handler.

### Negative
- Three layers for a single feature adds structural overhead compared to a handler-calls-database approach.
- The `MockIdentityRepo` stubs non-signup methods with default values, which could mask bugs if those methods are called unexpectedly.
- `create_device_key_with_executor` breaks the generic executor pattern by requiring `&mut PgConnection`, creating a minor inconsistency.

### Neutral
- The `IdentityService` trait currently has only one method (`signup`). Additional methods will be added as features like login and device management are implemented.
- Constraint-to-error mapping (e.g., `accounts_username_key` → `DuplicateUsername`) couples repo code to database constraint names, but this coupling is localized and tested.

## Alternatives considered

### Single handler function with inline validation
- Fewer files, less indirection
- Rejected because it makes isolated testing impossible — every test needs a database and real crypto
- Violates "make wrong code hard to write" — nothing prevents calling the repo with unvalidated data

### Separate trait per entity (AccountRepo, BackupRepo, DeviceKeyRepo)
- Considered in early design; the original implementation had three separate traits
- Rejected in [PR #329](https://github.com/icook/tiny-congress/pull/329) because `create_signup` needs all three in one transaction, making separate traits an awkward fit
- A consolidated trait with a compound `create_signup` method is simpler and directly expresses the atomic operation

### Pass `ValidatedSignup` fields individually to repo methods
- Avoids the `ValidatedSignup` struct entirely
- Rejected because it loses the "validation happened" guarantee — callers could pass arbitrary strings to repo methods

## References
- [ADR-008: Identity Model](008-identity-model.md) — the domain model this architecture implements
- [PR #322: Identity service layer](https://github.com/icook/tiny-congress/pull/322) — initial three-layer implementation
- [PR #329: Consolidate identity traits](https://github.com/icook/tiny-congress/pull/329) — refactor from three traits to one
- `service/src/identity/service.rs` — IdentityService trait and DefaultIdentityService
- `service/src/identity/repo/identity.rs` — IdentityRepo trait, ValidatedSignup, MockIdentityRepo
- `service/src/identity/http/mod.rs` — HTTP handler and error mapping
