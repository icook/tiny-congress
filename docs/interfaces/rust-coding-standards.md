# Rust Coding Standards

Guidelines for consistent, maintainable Rust code in the service crate.

## Error Handling

> **See also:** [Error Handling Patterns](./error-handling.md) for comprehensive guidance including standard error codes, REST/GraphQL response formats, and frontend integration.

### Use `thiserror` for Domain Errors

Define typed errors for each module rather than returning raw tuples or strings:

```rust
// Good: Typed error with thiserror
#[derive(Debug, thiserror::Error)]
pub enum AccountError {
    #[error("account not found: {0}")]
    NotFound(Uuid),
    #[error("username already taken")]
    DuplicateUsername,
    #[error("invalid signature: {0}")]
    InvalidSignature(String),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

// Bad: Raw tuple errors
fn create_account() -> Result<Account, (StatusCode, String)> { ... }
```

### HTTP Handler Error Mapping

Use `IntoResponse` implementations to map domain errors to HTTP responses:

```rust
impl IntoResponse for AccountError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match &self {
            Self::NotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
            Self::DuplicateUsername => (StatusCode::CONFLICT, self.to_string()),
            Self::InvalidSignature(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            Self::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal error".into()),
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}
```

### Error Propagation

- Use `?` operator with `#[from]` conversions for clean propagation
- Use `anyhow` only at application boundaries or in tests
- Never use `.unwrap()` or `.expect()` in library code; use them sparingly in main/tests

```rust
// Good: Propagate with ?
async fn get_account(pool: &PgPool, id: Uuid) -> Result<Account, AccountError> {
    sqlx::query_as("SELECT * FROM accounts WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?  // sqlx::Error -> AccountError via #[from]
        .ok_or(AccountError::NotFound(id))
}

// Bad: Manual mapping everywhere
async fn get_account(pool: &PgPool, id: Uuid) -> Result<Account, (StatusCode, String)> {
    sqlx::query_as(...)
        .fetch_optional(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "not found".into()))
}
```

## Function Size

### Prefer Focused Functions

Long functions aren't inherently bad—sometimes a linear sequence of steps is clearer than scattered helpers. However, consider extraction when:

- The same logic appears in multiple places
- A block of code has a clear name and single responsibility
- Testing a subsection in isolation would be valuable
- The function mixes unrelated concerns (validation, crypto, persistence)

```rust
// Fine: Long but linear and readable
pub async fn create_endorsement(...) -> Result<...> {
    // 80 lines of sequential steps that belong together
}

// Also fine: Extracted for reuse or testability
pub async fn create_endorsement(...) -> Result<...> {
    let validated = validate_endorsement(&request)?;
    let envelope = verify_endorsement_signature(&request.envelope)?;
    persist_endorsement(pool, &validated, &envelope).await
}
```

Use `#[allow(clippy::too_many_lines)]` when the length is justified—just ensure the function remains cohesive.

### Extract Private Helpers

Use private functions within the module for reusable logic:

```rust
// In http/accounts.rs
pub async fn signup(...) -> Result<...> { ... }
pub async fn get_account(...) -> Result<...> { ... }

// Private helpers
fn validate_username(username: &str) -> Result<(), AccountError> { ... }
fn verify_root_signature(envelope: &SignedEnvelope, pubkey: &[u8]) -> Result<(), AccountError> { ... }
```

## Module Organization

### One Concern Per File

```
service/src/identity/
├── mod.rs              # Re-exports public API
├── crypto/
│   ├── mod.rs          # Re-exports
│   ├── canonical.rs    # RFC 8785 canonicalization
│   ├── ed25519.rs      # Signing/verification
│   └── kid.rs          # Key ID derivation
├── http/
│   ├── mod.rs          # Router setup
│   ├── accounts.rs     # Account endpoints
│   ├── devices.rs      # Device endpoints
│   └── error.rs        # HTTP error types  <-- Centralized!
└── repo/
    ├── mod.rs
    └── event_store.rs  # Sigchain persistence
```

### Centralize HTTP Errors

Create a shared error module for HTTP handlers:

```rust
// http/error.rs
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error(transparent)]
    Account(#[from] AccountError),
    #[error(transparent)]
    Device(#[from] DeviceError),
    #[error(transparent)]
    Endorsement(#[from] EndorsementError),
    #[error("internal error")]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for ApiError { ... }
```

## Linting Configuration

### Required Clippy Lints

The following are enforced in `Cargo.toml`:

```toml
[lints.clippy]
pedantic = { level = "warn", priority = -1 }
nursery = { level = "warn", priority = -1 }
missing_errors_doc = "warn"
missing_panics_doc = "warn"
```

### Additional Recommended Lints

Consider enabling these for stricter enforcement:

```toml
[lints.clippy]
# Existing
pedantic = { level = "warn", priority = -1 }
nursery = { level = "warn", priority = -1 }
missing_errors_doc = "warn"
missing_panics_doc = "warn"

# Recommended additions
unwrap_used = "warn"           # Prefer ? or explicit error handling
expect_used = "warn"           # Same as above
panic = "warn"                 # No panics in library code
todo = "warn"                  # No TODO!() in production
unimplemented = "warn"         # No unimplemented!()
dbg_macro = "warn"             # No dbg!() in commits

# Allow these (already set)
module_name_repetitions = "allow"
missing_docs_in_private_items = "allow"
similar_names = "allow"
```

### Pre-commit Checks

Always run before committing:

```bash
cargo fmt --all
cargo clippy --all-features -- -D warnings
cargo test
```

## Async Patterns

### Use `async fn` Not `impl Future`

```rust
// Good
pub async fn fetch_account(pool: &PgPool, id: Uuid) -> Result<Account, AccountError> {
    ...
}

// Avoid unless necessary for lifetime reasons
pub fn fetch_account(pool: &PgPool, id: Uuid) -> impl Future<Output = Result<...>> {
    ...
}
```

### Transaction Handling

Use explicit transaction blocks with proper error handling:

```rust
pub async fn create_with_delegation(
    pool: &PgPool,
    account: &NewAccount,
    delegation: &Delegation,
) -> Result<Account, AccountError> {
    let mut tx = pool.begin().await?;

    let account = sqlx::query_as(...)
        .execute(&mut *tx)
        .await?;

    sqlx::query(...)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(account)
}
```

## Documentation

### Document Public APIs

All public functions should have doc comments:

```rust
/// Create a new account with initial device delegation.
///
/// # Errors
///
/// Returns `AccountError::DuplicateUsername` if the username is taken.
/// Returns `AccountError::InvalidSignature` if the delegation envelope fails verification.
pub async fn create_account(
    pool: &PgPool,
    request: CreateAccountRequest,
) -> Result<Account, AccountError> {
    ...
}
```

### Document Error Cases

Use the `# Errors` section to document when each error variant is returned.

## Testing

> **See also:** [Backend Testing Patterns](../playbooks/backend-test-patterns.md) for comprehensive testing guidance including database tests, mocking, and test infrastructure.

### Use `#[sqlx::test]` for DB Tests

```rust
#[sqlx::test]
async fn test_create_account(pool: PgPool) {
    // Pool is automatically set up with migrations
    let result = create_account(&pool, valid_request()).await;
    assert!(result.is_ok());
}
```

### Test Error Paths

Always test that errors are returned correctly:

```rust
#[sqlx::test]
async fn test_duplicate_username_rejected(pool: PgPool) {
    create_account(&pool, request_with_username("alice")).await.unwrap();

    let result = create_account(&pool, request_with_username("alice")).await;
    assert!(matches!(result, Err(AccountError::DuplicateUsername)));
}
```

## Anti-patterns

| Don't | Do Instead |
|-------|------------|
| `Result<T, (StatusCode, String)>` | Define typed error enum |
| `.unwrap()` in library code | Use `?` with proper error type |
| `println!` / `dbg!` | Use `tracing::debug!` |
| `panic!` in handlers | Return error variant |
| Inline SQL strings repeated | Use constants or query builder |
| `clone()` without need | Borrow or use references |
| String parsing for error detection | Use structured error types |

### Avoid String Parsing for Error Detection

Never match errors by parsing their `Display` output or searching for substrings. Error messages are not stable APIs—they can change between versions, vary by locale, or differ across database drivers.

```rust
// Bad: Fragile string matching
if e.to_string().contains("accounts_username_key") {
    return Err(ApiError::DuplicateUsername);
}

// Good: Structured error inspection
if let sqlx::Error::Database(db_err) = &e {
    if let Some(constraint) = db_err.constraint() {
        match constraint {
            "accounts_username_key" => return Err(ApiError::DuplicateUsername),
            "accounts_root_kid_key" => return Err(ApiError::DuplicateKey),
            _ => {}
        }
    }
}
```

This applies to all error handling—use typed error variants, error codes, or structured accessors rather than string matching.
