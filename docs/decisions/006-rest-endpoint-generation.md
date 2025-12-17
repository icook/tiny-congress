# ADR-006: REST Endpoint Generation Strategy

## Status
Proposed

## Context
Issue #156 requests adding REST endpoint generation alongside the existing GraphQL codegen workflow. This would enable broader integration options for clients that prefer REST over GraphQL.

The current architecture consists of:
- **Backend**: Rust/Axum with async-graphql v7.0
- **Codegen Pipeline**: Two-stage (Rust → GraphQL SDL → TypeScript + Zod)
- **Frontend**: React/TypeScript with TanStack Query

REST endpoints would need to:
1. Coexist with GraphQL without code duplication
2. Maintain the same type safety guarantees
3. Integrate with the existing codegen pipeline
4. Generate TypeScript types for frontend consumption

## Decision
**Recommended Approach: utoipa with code-first OpenAPI generation**

Add REST endpoints using the `utoipa` crate ecosystem, which generates OpenAPI 3.x specs from annotated Rust handlers. This approach:

1. Uses derive macros on existing types (same types as GraphQL)
2. Generates OpenAPI spec at build time
3. Extends the codegen pipeline to produce TypeScript types from OpenAPI
4. Provides Swagger UI for REST endpoint documentation

### Implementation Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Rust Types (Single Source)                  │
│   #[derive(SimpleObject, ToSchema)]                                │
│   struct BuildInfo { ... }                                          │
└─────────────────────────────────────────────────────────────────────┘
                │                              │
                ▼                              ▼
┌───────────────────────────┐    ┌───────────────────────────┐
│   async-graphql           │    │   utoipa                  │
│   Schema Export           │    │   OpenAPI Export          │
└───────────────────────────┘    └───────────────────────────┘
                │                              │
                ▼                              ▼
┌───────────────────────────┐    ┌───────────────────────────┐
│   schema.graphql          │    │   openapi.json            │
└───────────────────────────┘    └───────────────────────────┘
                │                              │
                ▼                              ▼
┌───────────────────────────┐    ┌───────────────────────────┐
│   graphql-codegen         │    │   openapi-typescript      │
│   → TypeScript + Zod      │    │   → TypeScript types      │
└───────────────────────────┘    └───────────────────────────┘
                │                              │
                └──────────────┬───────────────┘
                               ▼
                ┌───────────────────────────┐
                │   web/src/api/generated/  │
                │   - graphql.ts            │
                │   - rest.ts               │
                └───────────────────────────┘
```

### Required Dependencies

**Backend (Cargo.toml)**:
```toml
utoipa = { version = "5", features = ["axum_extras", "chrono", "uuid"] }
utoipa-axum = "0.2"
utoipa-swagger-ui = { version = "8", features = ["axum"] }
```

**Frontend (package.json)**:
```json
"openapi-typescript": "^7.0.0"
```

### Example Handler

```rust
use utoipa::OpenApi;

#[derive(utoipa::ToSchema, async_graphql::SimpleObject)]
pub struct BuildInfo {
    pub version: String,
    pub git_sha: String,
    pub build_time: String,
}

/// Get build information
#[utoipa::path(
    get,
    path = "/api/v1/build-info",
    responses(
        (status = 200, description = "Build information", body = BuildInfo)
    )
)]
async fn get_build_info() -> Json<BuildInfo> {
    // Implementation
}
```

### Extended Codegen Commands

```makefile
# justfile additions
export-openapi:
    cd service && cargo run --bin export_openapi > ../web/openapi.json

codegen-rest:
    cd web && npx openapi-typescript openapi.json -o src/api/generated/rest.ts

codegen: export-schema export-openapi codegen-frontend codegen-rest
```

## Consequences

### Positive
- Single source of truth: Rust types define both GraphQL and REST schemas
- Type safety maintained across entire stack
- OpenAPI spec enables third-party tooling (Postman, client generators)
- Swagger UI provides interactive REST documentation
- Familiar REST patterns for teams not versed in GraphQL
- Progressive adoption: can add REST endpoints incrementally

### Negative
- Additional derive macros on shared types (`ToSchema` alongside `SimpleObject`)
- Two codegen outputs to maintain (graphql.ts and rest.ts)
- Risk of API surface fragmentation if not carefully managed
- Swagger UI adds bundle size if served from same binary
- Learning curve for utoipa macro syntax

### Neutral
- REST endpoints live alongside GraphQL (both available on same server)
- Different URL patterns (/api/v1/* for REST, /graphql for GraphQL)
- May encourage creating REST-specific DTOs if response shapes diverge

## Alternatives Considered

### Alternative A: aide (Axum-native OpenAPI)
- **Description**: aide is designed specifically for Axum with tighter integration
- **Pros**: More idiomatic Axum routing, compile-time route validation
- **Cons**: Smaller ecosystem, less documentation, requires restructuring routes
- **Why not chosen**: utoipa has broader adoption and derive-macro approach aligns better with async-graphql patterns

### Alternative B: OpenAPI-first generation (progenitor)
- **Description**: Define OpenAPI spec first, generate Rust handlers from it
- **Pros**: Spec-first ensures API contract stability
- **Cons**: Doesn't share types with GraphQL, duplicate type definitions
- **Why not chosen**: Violates single-source-of-truth principle; would require maintaining separate type hierarchies

### Alternative C: REST facade over GraphQL
- **Description**: Auto-generate REST endpoints that proxy to GraphQL resolvers
- **Pros**: Zero duplication, single implementation
- **Cons**: Performance overhead, complex mapping, GraphQL leaks through REST
- **Why not chosen**: Adds latency and complexity; REST semantics don't map cleanly to GraphQL operations

### Alternative D: Defer to API gateway
- **Description**: Keep backend GraphQL-only; use Kong/AWS API Gateway for REST translation
- **Pros**: Separation of concerns, battle-tested infrastructure
- **Cons**: Operational complexity, another system to maintain, cost
- **Why not chosen**: Overkill for current scale; adds infrastructure dependency

## Design Decisions

### 1. Versioning: URL Path

REST endpoints use URL path versioning: `/api/v1/...`

- Matches industry standard (GitHub, Stripe, Twilio)
- Explicit version in every request aids debugging
- Load balancers and proxies can route by path
- When v2 arrives, v1 can coexist indefinitely

### 2. Errors: RFC 7807 Problem Details

REST errors use RFC 7807 format with an `extensions` field mapping to existing GraphQL error codes:

```json
{
  "type": "https://tinycongress.com/errors/validation",
  "title": "Validation Error",
  "status": 400,
  "detail": "Email format is invalid",
  "instance": "/api/v1/users",
  "extensions": {
    "code": "VALIDATION_ERROR",
    "field": "email"
  }
}
```

Error code mapping (from `api-contracts.md`):
| Code | HTTP Status |
|------|-------------|
| `UNAUTHENTICATED` | 401 |
| `FORBIDDEN` | 403 |
| `NOT_FOUND` | 404 |
| `VALIDATION_ERROR` | 400 |
| `INTERNAL_ERROR` | 500 |

### 3. Authentication: Shared Middleware

Both GraphQL and REST use identical authentication via shared Axum middleware:

```rust
let auth_layer = AuthLayer::new(token_validator);

let app = Router::new()
    .route("/graphql", post(graphql_handler))
    .nest("/api/v1", rest_routes())
    .layer(auth_layer);
```

- Same `Authorization: Bearer <token>` header
- Token validation and user context injection stays DRY
- Existing error codes apply to both APIs

### 4. Rate Limiting: Unified Bucket

Both APIs share the same rate limit bucket per user/IP (from `api-contracts.md`):

- Unauthenticated: 100 requests/minute per IP
- Authenticated: 1000 requests/minute per user
- Same `X-RateLimit-Limit`, `X-RateLimit-Remaining`, `X-RateLimit-Reset` headers

This prevents gaming by switching APIs and simplifies client documentation.

### 5. Deprecation: Sunset Header (RFC 8594)

Deprecated endpoints signal retirement via:

1. **OpenAPI schema**: `deprecated: true` on the endpoint
2. **HTTP headers** on every response:
   ```
   Sunset: Sat, 31 Dec 2025 23:59:59 GMT
   Deprecation: true
   Link: </api/v2/new-endpoint>; rel="successor-version"
   ```
3. **Documentation**: Changelog entry with migration guide
4. **Monitoring**: Log deprecated endpoint usage for tracking migration

This parallels GraphQL's `@deprecated(reason: "...")` directive using REST-native standards.

## Implementation Phases

### Phase 1: Foundation
- Add utoipa dependencies
- Create export_openapi binary
- Set up OpenAPI codegen for TypeScript
- Add /api/v1/build-info as proof of concept

### Phase 2: Core Endpoints
- Identify high-value endpoints for REST exposure
- Implement REST handlers sharing types with GraphQL
- Document REST API in Swagger UI

### Phase 3: Frontend Integration
- Create REST client utilities (similar to graphqlClient.ts)
- Add TanStack Query hooks for REST endpoints
- Update documentation

## References
- [Issue #156: Add REST endpoint generation to codegen](https://github.com/icook/tiny-congress-4/issues/156)
- [utoipa documentation](https://docs.rs/utoipa)
- [openapi-typescript](https://github.com/openapi-ts/openapi-typescript)
- [Axum REST patterns](https://docs.rs/axum/latest/axum/)
