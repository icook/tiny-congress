# Error Handling Patterns

Error handling should be **typed** (structured error types, not strings), **informative** (context for debugging without leaking internals), and **logged** (details captured server-side, user-friendly messages shown to clients).

## Error codes

Standard error codes are defined in [api-contracts.md](./api-contracts.md#error-codes). Use those as the single source of truth.

## Platform-specific guides

- [Backend (Rust)](./error-handling-backend.md) — thiserror patterns, HTTP responses, GraphQL errors
- [Frontend (React)](./error-handling-frontend.md) — Error boundaries, network errors, form validation

## See also

- [Rust Coding Standards](./rust-coding-standards.md) — Error handling section
- [React Coding Standards](./react-coding-standards.md) — Error boundary section
