# Backend (Rust) Error Handling

Rust-specific error handling patterns for the service layer. For general error codes and concepts, see [Error Handling Patterns](./error-handling.md).

## REST Error Response Format

All REST error responses use a single shared struct:

```rust
// service/src/http/mod.rs
pub struct ErrorResponse {
    pub error: String,
}
```

Wire format — all errors look like this:

```json
{ "error": "Username already taken" }
```

## Shared Helper Functions

Import from `crate::http`. These are the **only** correct way to build error responses in handlers.

| Helper | Status | Signature |
|---|---|---|
| `bad_request` | 400 | `bad_request(msg: &str) -> Response` |
| `unauthorized` | 401 | `unauthorized(msg: &str) -> Response` |
| `forbidden` | 403 | `forbidden(msg: &str) -> Response` |
| `not_found` | 404 | `not_found(msg: &str) -> Response` |
| `conflict` | 409 | `conflict(msg: &str) -> Response` |
| `too_many_requests` | 429 | `too_many_requests(msg: &str) -> Response` |
| `internal_error` | 500 | `internal_error() -> Response` |

`internal_error` takes no message — it always returns `"Internal server error"` to avoid leaking details.

### Usage in handlers

```rust
use crate::http::{bad_request, not_found, unauthorized, internal_error};

async fn get_account(
    Path(id): Path<Uuid>,
    Extension(repo): Extension<Arc<dyn AccountRepo>>,
) -> axum::response::Response {
    match repo.find(id).await {
        Ok(account) => Json(account).into_response(),
        Err(AccountRepoError::NotFound(_)) => not_found("account not found"),
        Err(e) => {
            tracing::error!("database error: {e}");
            internal_error()
        }
    }
}
```

### What not to do

`just lint-patterns` mechanically rejects these patterns:

```rust
// BAD: inline construction — blocked by lint-patterns
(StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "...".into() })).into_response()

// BAD: json macro — blocked by lint-patterns
serde_json::json!({"error": "..."})
```

`conflict`, `forbidden`, and `too_many_requests` are not covered by the lint check (they are less commonly misused) but should still use the shared helpers.

## Domain Error Types

Use `thiserror` for typed domain errors:

```rust
#[derive(Debug, thiserror::Error)]
pub enum AccountRepoError {
    #[error("username already taken")]
    DuplicateUsername,

    #[error("account not found: {0}")]
    NotFound(Uuid),

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}
```

Map domain errors to HTTP responses explicitly in the handler. Do not implement `IntoResponse` on repo error types — the same error can map to different status codes depending on context (e.g., `DuplicateUsername` during signup is a user error; during an internal migration step it is a 500).

## Logging vs Returning Errors

| Scenario | Log level | Response message |
|---|---|---|
| Validation failure | `debug!` | Full error message |
| Business rule violation | `info!` | User-friendly message |
| Database error | `error!` | `internal_error()` — no details |
| External service failure | `warn!` | `internal_error()` or retry message |

Never expose SQL errors, stack traces, or internal identifiers in `error` response bodies.

## Structured Error Inspection

Match on typed error variants, not string output:

```rust
// Good
if let sqlx::Error::Database(db_err) = &e {
    if let Some("accounts_username_key") = db_err.constraint() {
        return conflict("username already taken");
    }
}

// Bad — fragile
if e.to_string().contains("unique constraint") { ... }
```

## GraphQL Error Responses

GraphQL (currently a stub — `buildInfo` only) uses `async_graphql::Error`:

```rust
repo.find(id).await.map_err(|e| match e {
    AccountRepoError::NotFound(_) => async_graphql::Error::new("not found"),
    _ => async_graphql::Error::new("internal error"),
})?;
```

All feature work uses REST. GraphQL error handling is not exercised in production paths.

## Adding a New Helper

If a new status code is needed frequently enough to warrant a helper, add it to `service/src/http/mod.rs` following the existing pattern (all helpers are `#[must_use]` and take `msg: &str` except `internal_error`). Update the table in this document and in `AGENTS.md`.

---

## See Also

- [Error Handling Patterns](./error-handling.md) — Overview and standard error codes
- [Frontend Error Handling](./error-handling-frontend.md) — React error boundaries and network errors
- [Rust Coding Standards](./rust-coding-standards.md) — General Rust conventions
