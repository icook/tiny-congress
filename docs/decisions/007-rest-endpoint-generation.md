# ADR-007: REST Endpoint Generation Strategy

## Status
Accepted (Phase 1 implemented)

## Context
Adding REST endpoint generation alongside GraphQL to enable broader integration options for clients that prefer REST. REST endpoints must coexist with GraphQL without code duplication, maintain the same type safety guarantees, and integrate with the existing codegen pipeline.

## Decision
Use `utoipa` with code-first OpenAPI generation. Rust types are the single source of truth — `#[derive(ToSchema)]` alongside `#[derive(SimpleObject)]` generates OpenAPI specs from the same types used for GraphQL.

**Architecture:**
- Shared Rust types → async-graphql (GraphQL SDL) + utoipa (OpenAPI JSON)
- Each spec feeds its own codegen pipeline for TypeScript types
- REST endpoints live at `/api/v1/*`, GraphQL at `/graphql`

**Key implementation files:**
- `service/src/rest.rs` — REST handlers, `ProblemDetails` (RFC 7807), OpenAPI doc
- `service/src/bin/export_openapi.rs` — OpenAPI JSON export for codegen
- `service/src/build_info.rs` — `BuildInfo` type with both `ToSchema` and `SimpleObject` derives

**Design decisions:**
- URL path versioning (`/api/v1/...`)
- RFC 7807 Problem Details for errors, with `extensions.code` mapping to GraphQL error codes
- Shared auth middleware for both GraphQL and REST
- Unified rate limit bucket across both APIs
- RFC 8594 Sunset headers for deprecation

## Consequences

### Positive
- Single source of truth for types across GraphQL and REST
- OpenAPI spec enables third-party tooling (Postman, client generators)
- Progressive adoption: REST endpoints added incrementally

### Negative
- Additional derive macros on shared types
- Two codegen outputs to maintain
- Risk of API surface fragmentation if not carefully managed

## Alternatives Considered
- **aide** (Axum-native OpenAPI) — smaller ecosystem, less documentation
- **progenitor** (spec-first) — violates single-source-of-truth; separate type hierarchies
- **REST facade over GraphQL** — performance overhead, leaky abstraction
- **API gateway** — overkill for current scale
