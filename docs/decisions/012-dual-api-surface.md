# ADR-012: Dual API Surface (GraphQL + REST)

## Status
Accepted

## Context

TinyCongress needs both graph-structured queries (e.g., nested delegation chains, member relationships) and simple request-response operations (e.g., cryptographic signup, device key management). A single API style forces awkward tradeoffs: GraphQL is excellent for flexible queries but poor for operations that map naturally to HTTP status codes and structured error responses. REST is excellent for CRUD with clear success/failure semantics but poor for queries that require multiple round trips or variable-depth traversal.

Several tensions shaped this decision:

- **Type safety across the stack.** Both APIs generate TypeScript types for the frontend. Two separate type generation pipelines must stay in sync with the Rust source types — divergence is a bug.
- **Shared middleware.** Authentication, CORS, rate limiting, and security headers apply uniformly regardless of which API surface handles the request. The routing architecture must support shared middleware without duplication.
- **Developer tools in development, locked down in production.** GraphQL Playground and Swagger UI are essential during development but expose schema and API documentation to potential attackers in production.

## Decision

### Same binary, two API surfaces

A single Axum binary serves both APIs:

- **GraphQL** at `/graphql` via `async-graphql` — for query-heavy, graph-structured data
- **REST** at `/api/v1/*` via Axum with `utoipa` (OpenAPI code-first generation) — for identity, auth, and structured operations

Both share the same middleware stack (CORS, security headers, extensions for database pool and services).

### Endpoint assignment criteria

**REST endpoints** are used for operations that:
- Have meaningful HTTP status code semantics (201 Created, 409 Conflict, 422 Unprocessable)
- Handle binary or cryptographic payloads (base64url-encoded keys, backup envelopes)
- Benefit from structured error responses with field-level detail
- Map to a single resource lifecycle (create account, register device, retrieve backup)

Current REST endpoints:
- `POST /auth/signup` — account creation with cryptographic identity
- `GET /api/v1/build-info` — build metadata

**GraphQL endpoints** are used for operations that:
- Require flexible field selection or nested data traversal
- Serve read-heavy dashboards with variable data requirements
- Benefit from query batching

Current GraphQL operations:
- `buildInfo` query — build metadata (also available via REST for tooling compatibility)
- `echo` mutation — placeholder for development

### RFC 7807 Problem Details for REST errors

REST endpoints return errors in RFC 7807 format:

```rust
pub struct ProblemDetails {
    pub problem_type: String,  // URI: "https://tinycongress.com/errors/<category>"
    pub title: String,         // Human-readable summary
    pub status: StatusCode,    // HTTP status code (serialized as u16)
    pub detail: String,        // Specific occurrence explanation
    pub instance: Option<String>,
    pub extensions: Option<ProblemExtensions>,
}

pub struct ProblemExtensions {
    pub code: String,          // Maps to GraphQL error codes (e.g., "VALIDATION_ERROR")
    pub field: Option<String>, // Optional field reference
}
```

The `extensions.code` field uses the same error code vocabulary as GraphQL errors (`VALIDATION_ERROR`, `INTERNAL_ERROR`, `UNAUTHENTICATED`, `FORBIDDEN`, `NOT_FOUND`), ensuring consistent error handling across both APIs on the frontend.

### Shared type derivation

Rust types derive both GraphQL and OpenAPI annotations:

```rust
#[derive(SimpleObject, ToSchema, Serialize)]
pub struct BuildInfo {
    pub version: String,
    pub git_sha: String,
    pub build_time: String,
}
```

This single source of truth generates:
- GraphQL SDL (via `export_schema` binary → `web/schema.graphql`)
- OpenAPI spec (via `export_openapi` binary → `web/openapi.json`)
- TypeScript types from both specs via separate codegen pipelines (see [ADR-013](013-frontend-architecture.md))

### Developer tools disabled by default

Both interactive tools are controlled by configuration (see [ADR-011](011-figment-layered-configuration.md)):

- **GraphQL Playground** (`GET /graphql`): disabled by default. Enabled via `TC_GRAPHQL__PLAYGROUND_ENABLED=true`. Serves the async-graphql Playground IDE for query exploration.
- **Swagger UI** (`/swagger-ui`): disabled by default. Enabled via `TC_SWAGGER__ENABLED=true`. Serves the OpenAPI spec via `utoipa-swagger-ui`.

Both log their state at startup, making it clear whether they're active.

### Router composition

In `main.rs`, routes are assembled in a single Axum `Router`:

1. `/graphql` — POST always enabled; GET conditionally serves Playground
2. `/api/v1/*` — REST endpoints nested under version prefix
3. `/auth/*` — Identity endpoints merged at root (not versioned — auth paths are stable)
4. `/health` — Health check
5. `/swagger-ui` — conditionally merged when enabled

Shared layers applied outermost-first: security headers → CORS → route-specific handlers. Extensions (database pool, GraphQL schema, identity service, build info) are added as Axum `Extension`s accessible to both API surfaces.

## Consequences

### Positive
- Each operation uses the API style that best fits its semantics. Signup with HTTP status codes; delegation queries with GraphQL field selection.
- RFC 7807 gives REST consumers structured, machine-readable errors with consistent error codes shared with GraphQL.
- Shared Rust types eliminate the risk of GraphQL and REST type definitions diverging.
- A single binary simplifies deployment — no need to route traffic to separate services.
- Disabling developer tools by default prevents accidental schema exposure in production.

### Negative
- Two API surfaces means two sets of client code, two codegen pipelines, and two styles of error handling on the frontend.
- The `/auth/signup` endpoint is mounted at root level, not under `/api/v1`, creating an inconsistency in URL structure. This was a deliberate choice (auth paths should be stable across API versions) but requires documentation.
- Developers must decide which surface each new endpoint belongs on. The criteria above help, but edge cases require judgment.

### Neutral
- `BuildInfo` is available on both surfaces — REST for health-check tooling, GraphQL for dashboard queries. This intentional overlap keeps both APIs useful independently.
- The OpenAPI spec declares `servers: [{ url: "/api/v1" }]`, scoping generated client paths. Identity endpoints at `/auth/*` are outside this scope and have separate TypeScript client code.

## Alternatives considered

### GraphQL only
- Simpler — single API surface, single codegen pipeline
- Rejected because GraphQL error handling is poorly suited to identity operations. A signup that fails due to duplicate username returns HTTP 200 with an `errors` array — the frontend must parse error extensions to determine the failure type. HTTP 409 Conflict is more natural and more interoperable with non-JavaScript clients.
- GraphQL mutations for crypto payloads (base64url-encoded keys, binary envelopes) feel forced

### REST only
- Simpler — standard HTTP semantics throughout
- Rejected because graph-structured queries (nested delegations, member relationships with votes) would require multiple round trips or complex query parameter schemes. GraphQL's flexible field selection is a genuine advantage for dashboard-style UIs.

### Separate services for GraphQL and REST
- Clearer separation of concerns
- Rejected for deployment complexity — two services to deploy, monitor, and version for a small team. A single binary with shared middleware is simpler and ensures consistent authentication/authorization.

### gRPC for internal operations
- Strong typing, code generation, efficient binary protocol
- Rejected because the primary consumer is a browser — gRPC-Web adds complexity and the browser doesn't benefit from binary efficiency for the request volumes TinyCongress handles.

## References
- [ADR-007: REST Endpoint Generation Strategy](007-rest-endpoint-generation.md) — rationale for utoipa and code-first OpenAPI
- [ADR-011: Figment Layered Configuration](011-figment-layered-configuration.md) — playground/swagger config controls
- [ADR-013: Frontend Architecture](013-frontend-architecture.md) — dual codegen consuming both API specs
- [RFC 7807: Problem Details for HTTP APIs](https://www.rfc-editor.org/rfc/rfc7807)
- [PR #199: Dual API surface](https://github.com/icook/tiny-congress/pull/199) — initial implementation
- `service/src/graphql.rs` — GraphQL schema, handler, playground
- `service/src/rest.rs` — ProblemDetails, ApiDoc, REST handlers
- `service/src/identity/http/mod.rs` — Identity REST endpoints
- `service/src/main.rs` — router composition
- `docs/interfaces/api-contracts.md` — shared error code vocabulary
