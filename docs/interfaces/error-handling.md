# Error Handling Patterns

Comprehensive error handling guidelines for backend (Rust) and frontend (React) code.

## Overview

Error handling should be:
- **Typed**: Use structured error types, not strings
- **Informative**: Provide context for debugging without leaking internals
- **Recoverable**: Allow users to understand and recover from errors
- **Logged**: Capture details for debugging while showing user-friendly messages

## Standard Error Codes

Use consistent error codes across REST and GraphQL APIs:

| Code | HTTP Status | Description | When to Use |
|------|-------------|-------------|-------------|
| `INTERNAL_ERROR` | 500 | Unexpected server error | Database failures, panics, unhandled exceptions |
| `VALIDATION_ERROR` | 400 | Invalid input data | Missing fields, format errors, constraint violations |
| `NOT_FOUND` | 404 | Resource doesn't exist | Entity lookup failures |
| `CONFLICT` | 409 | Resource conflict | Duplicate username, concurrent modification |
| `UNAUTHORIZED` | 401 | Authentication required | Missing or invalid credentials |
| `FORBIDDEN` | 403 | Permission denied | Insufficient privileges |
| `RATE_LIMITED` | 429 | Too many requests | Rate limit exceeded |

### Error Code Format

Use SCREAMING_SNAKE_CASE for error codes:

```
DUPLICATE_USERNAME
INVALID_SIGNATURE
ACCOUNT_NOT_FOUND
```

---

## Platform-Specific Guides

- [Backend (Rust) Error Handling](./error-handling-backend.md) - thiserror patterns, HTTP responses, GraphQL errors
- [Frontend (React) Error Handling](./error-handling-frontend.md) - Error boundaries, network errors, form validation

---

## Localization Considerations

### Backend

Return error codes, not messages, for client-side translation:

```json
{
  "error": {
    "code": "DUPLICATE_USERNAME",
    "field": "username"
  }
}
```

### Frontend

Map error codes to localized messages:

```tsx
const errorMessages: Record<string, string> = {
  DUPLICATE_USERNAME: t('errors.duplicateUsername'),
  VALIDATION_ERROR: t('errors.validation'),
  INTERNAL_ERROR: t('errors.internal'),
};

function getErrorMessage(code: string, fallback: string): string {
  return errorMessages[code] ?? fallback;
}
```

---

## See Also

- [Rust Coding Standards](./rust-coding-standards.md) - Error handling section
- [React Coding Standards](./react-coding-standards.md) - Error boundary section
- [API Contracts](./api-contracts.md) - Error response formats
- `service/src/rest.rs` - ProblemDetails implementation
- `web/src/components/ErrorBoundary/` - React error boundary components
