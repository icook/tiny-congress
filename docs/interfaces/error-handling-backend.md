# Backend (Rust) Error Handling

Rust-specific error handling patterns for the service layer. For general error codes and concepts, see [Error Handling Patterns](./error-handling.md).

## Error Type Hierarchy

Organize errors into domain and infrastructure categories:

```
Domain Errors (business logic)
├── AccountError
│   ├── NotFound
│   ├── DuplicateUsername
│   └── InvalidSignature
├── EndorsementError
│   ├── InvalidSignature
│   └── ExpiredTimestamp
└── AuthError
    ├── InvalidToken
    └── Expired

Infrastructure Errors (technical)
├── DatabaseError
├── NetworkError
└── ConfigError
```

## Using `thiserror` for Domain Errors

Define typed errors for each module:

```rust
// service/src/identity/repo/accounts.rs
#[derive(Debug, thiserror::Error)]
pub enum AccountRepoError {
    #[error("username already taken")]
    DuplicateUsername,

    #[error("public key already registered")]
    DuplicateKey,

    #[error("account not found: {0}")]
    NotFound(Uuid),

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}
```

**Key principles:**
- Each variant represents a specific failure mode
- Use `#[from]` for automatic conversion from underlying errors
- Include relevant context (IDs, field names) in variants

## HTTP Error Responses

### REST API: RFC 7807 Problem Details

Use the `ProblemDetails` struct for REST endpoints:

```rust
// service/src/rest.rs
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProblemDetails {
    #[serde(rename = "type")]
    pub problem_type: String,    // URI identifying the error type
    pub title: String,           // Short summary
    pub status: u16,             // HTTP status code
    pub detail: String,          // Human-readable explanation
    pub instance: Option<String>, // URI for this occurrence
    pub extensions: Option<ProblemExtensions>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ProblemExtensions {
    pub code: String,            // Standard error code
    pub field: Option<String>,   // Field that caused error (validation)
}
```

Example response:

```json
{
  "type": "https://tinycongress.com/errors/validation",
  "title": "Validation Error",
  "status": 400,
  "detail": "Username must be between 3 and 64 characters",
  "extensions": {
    "code": "VALIDATION_ERROR",
    "field": "username"
  }
}
```

### Handler Error Mapping

Map domain errors to HTTP responses using `IntoResponse`:

```rust
// service/src/identity/http/mod.rs
impl IntoResponse for AccountRepoError {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::DuplicateUsername => (
                StatusCode::CONFLICT,
                Json(ErrorResponse { error: "Username already taken".to_string() }),
            ).into_response(),

            Self::DuplicateKey => (
                StatusCode::CONFLICT,
                Json(ErrorResponse { error: "Public key already registered".to_string() }),
            ).into_response(),

            Self::NotFound(id) => (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse { error: format!("Account not found: {id}") }),
            ).into_response(),

            Self::Database(e) => {
                tracing::error!("Database error: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse { error: "Internal server error".to_string() }),
                ).into_response()
            }
        }
    }
}
```

**Important:** Never expose internal error details (SQL errors, stack traces) in responses.

## GraphQL Error Responses

For GraphQL, use `async_graphql::Error` with extensions:

```rust
use async_graphql::{Error, ErrorExtensions};

async fn get_account(&self, ctx: &Context<'_>, id: Uuid) -> Result<Account> {
    let account = repo.find(id).await.map_err(|e| match e {
        AccountRepoError::NotFound(_) => Error::new("Account not found")
            .extend_with(|_, e| {
                e.set("code", "NOT_FOUND");
                e.set("id", id.to_string());
            }),
        _ => Error::new("Internal error")
            .extend_with(|_, e| e.set("code", "INTERNAL_ERROR")),
    })?;
    Ok(account)
}
```

Response format:

```json
{
  "data": null,
  "errors": [{
    "message": "Account not found",
    "locations": [{"line": 1, "column": 1}],
    "path": ["getAccount"],
    "extensions": {
      "code": "NOT_FOUND",
      "id": "550e8400-e29b-41d4-a716-446655440000"
    }
  }]
}
```

## Error Propagation

Use the `?` operator with `#[from]` conversions:

```rust
// Good: Clean propagation
async fn create_account(pool: &PgPool, req: CreateRequest) -> Result<Account, AccountError> {
    let validated = validate_request(&req)?;  // ValidationError -> AccountError
    let account = repo.create(&validated).await?;  // sqlx::Error -> AccountError
    Ok(account)
}

// Bad: Manual mapping everywhere
async fn create_account(pool: &PgPool, req: CreateRequest) -> Result<Account, String> {
    let validated = validate_request(&req)
        .map_err(|e| format!("validation failed: {e}"))?;
    // ...
}
```

## Logging vs Returning Errors

| Scenario | Log Level | Return to Client |
|----------|-----------|------------------|
| Validation failure | `debug!` | Full error message |
| Business rule violation | `info!` | User-friendly message |
| Database error | `error!` | Generic "Internal error" |
| External service failure | `warn!` | Retry message or generic error |

```rust
match repo.create(account).await {
    Ok(account) => Ok(Json(account)),
    Err(AccountRepoError::DuplicateUsername) => {
        tracing::debug!("Duplicate username attempt: {}", account.username);
        Err((StatusCode::CONFLICT, "Username already taken"))
    }
    Err(AccountRepoError::Database(e)) => {
        tracing::error!("Database error creating account: {e}");
        Err((StatusCode::INTERNAL_SERVER_ERROR, "Internal error"))
    }
}
```

## Structured Error Inspection

Never match errors by parsing string output:

```rust
// Bad: String matching is fragile
if e.to_string().contains("unique constraint") {
    return Err(ApiError::DuplicateUsername);
}

// Good: Use structured error accessors
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

## Testing Error Handling

```rust
#[sqlx::test]
async fn test_duplicate_username_returns_conflict(pool: PgPool) {
    // Create first account
    create_account(&pool, request_with_username("alice")).await.unwrap();

    // Attempt duplicate
    let result = create_account(&pool, request_with_username("alice")).await;
    assert!(matches!(result, Err(AccountError::DuplicateUsername)));
}

#[tokio::test]
async fn test_error_response_format() {
    let app = TestAppBuilder::with_mocks().build();

    let response = app.oneshot(/* invalid request */).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body: ErrorResponse = parse_body(response).await;
    assert!(!body.error.is_empty());
}
```

---

## See Also

- [Error Handling Patterns](./error-handling.md) - Overview and standard error codes
- [Frontend Error Handling](./error-handling-frontend.md) - React error boundaries and network errors
- [Rust Coding Standards](./rust-coding-standards.md) - General Rust conventions
