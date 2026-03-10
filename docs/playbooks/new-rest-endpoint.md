# Adding a New REST Endpoint

## When to use
- Adding a single-resource operation that benefits from meaningful HTTP status codes (201, 409, 422)
- Binary or cryptographic payloads
- Endpoints needing field-level error detail
- See [ADR-012](../decisions/012-dual-api-surface.md) for REST vs GraphQL decision criteria

## Prerequisites
- Backend compiles: `just build-backend`
- Understanding of the three-layer architecture: HTTP handler → Service → Repository ([ADR-016](../decisions/016-repo-service-http-architecture.md))

## Steps

### 1. Define request and response types

Place types in the module's `http/mod.rs` (or a dedicated submodule if the handler is complex).

Request types derive `Deserialize` + `ToSchema`. Response types derive `Serialize` + `ToSchema`:

```rust
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateWidgetRequest {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreatedWidgetResponse {
    pub id: Uuid,
    pub name: String,
    pub created_at: String,
}
```

If a type is shared with GraphQL, also derive `SimpleObject`:
```rust
#[derive(Debug, Serialize, Deserialize, SimpleObject, ToSchema)]
pub struct Widget { ... }
```

### 2. Add repository method

Add to the appropriate repo trait and implement with sqlx:

```rust
// In the repo trait
async fn create_widget(&self, name: &str, desc: Option<&str>) -> Result<WidgetRecord, WidgetRepoError>;

// In the Pg implementation
async fn create_widget(&self, name: &str, desc: Option<&str>) -> Result<WidgetRecord, WidgetRepoError> {
    create_widget_with_executor(&self.pool, name, desc).await
}
```

Use the executor-generic pattern if the operation participates in transactions:
```rust
pub async fn create_widget_with_executor<'e, E>(
    executor: E,
    name: &str,
    desc: Option<&str>,
) -> Result<WidgetRecord, WidgetRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_as!(WidgetRecord, "INSERT INTO widgets ...")
        .fetch_one(executor)
        .await
        .map_err(|e| /* map sqlx errors */)
}
```

### 3. Add service method (if business logic is needed)

The service layer owns validation and orchestration. Keep handlers thin (~15 lines):

```rust
#[async_trait]
pub trait WidgetService: Send + Sync {
    async fn create_widget(&self, req: &CreateWidgetRequest) -> Result<WidgetRecord, WidgetError>;
}

#[derive(Debug, thiserror::Error)]
pub enum WidgetError {
    #[error("{0}")]
    Validation(String),
    #[error("Widget name already taken")]
    DuplicateName,
    #[error("internal error: {0}")]
    Internal(String),
}
```

If there is no business logic beyond CRUD, you can call the repo directly from the handler.

### 4. Write the HTTP handler

Two patterns exist — pick based on complexity:

**Pattern A: `Result<Json<T>, ProblemDetails>`** — for simple handlers with few error cases:
```rust
#[utoipa::path(
    get,
    path = "/widgets/{id}",
    tag = "Widgets",
    responses(
        (status = 200, description = "Widget found", body = WidgetResponse),
        (status = 404, description = "Widget not found", body = ProblemDetails),
        (status = 500, description = "Internal server error", body = ProblemDetails)
    )
)]
pub async fn get_widget(
    Extension(repo): Extension<Arc<dyn WidgetRepo>>,
    Path(id): Path<Uuid>,
) -> Result<Json<WidgetResponse>, ProblemDetails> {
    // ...
}
```

**Pattern B: `impl IntoResponse`** — for handlers with domain-specific error mapping:
```rust
#[utoipa::path(
    post,
    path = "/widgets",
    tag = "Widgets",
    request_body = CreateWidgetRequest,
    responses(
        (status = 201, description = "Widget created", body = CreatedWidgetResponse),
        (status = 400, description = "Validation error"),
        (status = 409, description = "Duplicate name"),
        (status = 500, description = "Internal server error")
    )
)]
async fn create_widget(
    Extension(service): Extension<Arc<dyn WidgetService>>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    let req: CreateWidgetRequest = match auth.json() {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    match service.create_widget(&req).await {
        Ok(record) => (StatusCode::CREATED, Json(CreatedWidgetResponse {
            id: record.id,
            name: record.name,
            created_at: record.created_at.to_rfc3339(),
        })).into_response(),
        Err(e) => widget_error_response(e),
    }
}

fn widget_error_response(e: WidgetError) -> axum::response::Response {
    match e {
        WidgetError::Validation(msg) => {
            (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: msg })).into_response()
        }
        WidgetError::DuplicateName => {
            (StatusCode::CONFLICT, Json(ErrorResponse { error: "Widget name already taken".into() })).into_response()
        }
        WidgetError::Internal(ref msg) => {
            tracing::error!("Widget error: {msg}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Internal error".into() })).into_response()
        }
    }
}
```

**Authentication:** For endpoints requiring auth, use the `AuthenticatedDevice` extractor. It verifies Ed25519 signatures, checks nonce replay, and provides `auth.account_id` and `auth.device_kid`. Deserialize the body via `auth.json::<T>()` (not `Json<T>` — the body bytes are consumed during signature verification).

### 5. Wire the route in the module router

Each module exports a `router()` function. Add your route there:

```rust
pub fn router() -> Router {
    Router::new()
        .route("/widgets", get(list_widgets).post(create_widget))
        .route("/widgets/{id}", get(get_widget).delete(delete_widget))
}
```

Then merge or nest in `main.rs`:
- **Nest under `/api/v1`** for versioned resource endpoints
- **Merge at root** for auth-related endpoints (existing pattern)

### 6. Register with OpenAPI

Add the handler and schema types to `ApiDoc` in `service/src/rest.rs`:

```rust
#[derive(OpenApi)]
#[openapi(
    // ... existing config ...
    paths(
        get_build_info,
        crate::reputation::http::create_endorsement_as_verifier,
        crate::widgets::http::create_widget,  // ← add handler
    ),
    components(schemas(
        // ... existing schemas ...
        crate::widgets::http::CreateWidgetRequest,    // ← add types
        crate::widgets::http::CreatedWidgetResponse,
    ))
)]
pub struct ApiDoc;
```

### 7. Regenerate TypeScript types

```bash
just codegen
```

This runs `export_openapi` → `web/openapi.json` → `openapi-typescript` → `web/src/api/generated/rest.ts`.

### 8. Add integration test

```rust
use tc_test_macros::shared_runtime_test;
use common::app_builder::TestAppBuilder;
use common::test_db::isolated_db;
use common::factories::{signup_user, build_authed_request};
use axum::{body::to_bytes, http::{Method, StatusCode}};
use tower::ServiceExt;

#[shared_runtime_test]
async fn test_create_widget_success() {
    let (app, keys, _db) = signup_user("widgetuser").await;

    let body = serde_json::json!({
        "name": "My Widget",
        "description": "A test widget"
    });

    let req = build_authed_request(
        Method::POST,
        "/widgets",
        &body.to_string(),
        &keys.device_signing_key,
        &keys.device_kid,
    );

    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::CREATED);

    let body = to_bytes(response.into_body(), 1024 * 1024).await.expect("body");
    let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(json["name"], "My Widget");
}
```

**Test infrastructure:**
- `#[shared_runtime_test]` — async test macro with shared Tokio runtime
- `isolated_db()` — spins up a test database via testcontainers
- `TestAppBuilder::new()` — builds a router with required extensions
- `signup_user(name)` — creates a real account with signing keys
- `build_authed_request(method, path, body, key, kid)` — constructs a signed request
- `tower::ServiceExt::oneshot()` — sends a single request through the router

See [Backend Test Patterns](./backend-test-patterns.md) for more detail.

### 9. Regenerate sqlx cache (if SQL changed)

```bash
cd service && cargo sqlx prepare
```

## Verification
- [ ] `just test-backend` passes
- [ ] `just lint-backend` clean
- [ ] `just codegen` produces no additional changes
- [ ] New endpoint visible in Swagger UI (enable with `TC_SWAGGER__ENABLED=true`)
- [ ] Frontend integration works with generated types

## Status code reference

| HTTP Code | Use Case | Error Code |
|-----------|----------|-----------|
| 200 OK | Successful read | — |
| 201 CREATED | Resource created | — |
| 400 BAD_REQUEST | Validation failure | VALIDATION_ERROR |
| 401 UNAUTHORIZED | Missing/invalid auth | UNAUTHENTICATED |
| 403 FORBIDDEN | Authenticated but not authorized | FORBIDDEN |
| 404 NOT_FOUND | Resource doesn't exist | NOT_FOUND |
| 409 CONFLICT | Duplicate resource | VALIDATION_ERROR |
| 422 UNPROCESSABLE_ENTITY | Business rule violation | VALIDATION_ERROR |
| 500 INTERNAL_SERVER_ERROR | Server error (safe message only) | INTERNAL_ERROR |

## Common failures

| Error | Cause | Fix |
|-------|-------|-----|
| Type not in OpenAPI spec | Missing `ToSchema` derive or not registered in `ApiDoc` | Add derive and register in `rest.rs` |
| 401 on authenticated endpoint | Signature verification failing | Check canonical message format: `{METHOD}\n{PATH}\n{TIMESTAMP}\n{NONCE}\n{BODY_SHA256}` |
| Body deserialization fails | Using `Json<T>` with `AuthenticatedDevice` | Use `auth.json::<T>()` instead — body is consumed during auth |
| Handler not reachable | Route not merged in `main.rs` | Check `.merge(module::http::router())` or `.nest("/api/v1", ...)` |
| `just codegen` shows diff | Forgot to run after adding `ToSchema` types | Run `just codegen` and commit the generated files |

## Prohibited actions
- DO NOT delete or rename existing public endpoints without deprecation
- DO NOT expose internal IDs without authorization checks
- DO NOT return raw error details from service/repo layer to clients (log them, return safe messages)
- DO NOT use `Json<T>` extractor on authenticated endpoints (use `auth.json::<T>()`)

## See also
- [ADR-012](../decisions/012-dual-api-surface.md) — REST vs GraphQL decision criteria
- [ADR-016](../decisions/016-repo-service-http-architecture.md) — three-layer architecture
- [ADR-007](../decisions/007-rest-endpoint-generation.md) — OpenAPI code-first strategy
- [Adding a GraphQL Endpoint](./new-graphql-endpoint.md) — parallel guide for GraphQL
- [Backend Test Patterns](./backend-test-patterns.md) — test infrastructure details
- [Test Writing Skill](../skills/test-writing.md) — LLM decision tree for test placement
- `service/src/rest.rs` — ProblemDetails, ApiDoc definition
- `service/src/identity/http/auth.rs` — AuthenticatedDevice extractor
