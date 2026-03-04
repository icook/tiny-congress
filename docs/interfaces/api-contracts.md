# API Contracts

## GraphQL endpoint

**URL:** `/graphql` (POST)

**Headers:**
```
Content-Type: application/json
```

### REST endpoint authentication (device-key signing)

Authenticated REST endpoints use Ed25519 request signing instead of bearer tokens:

```
X-Device-Kid: <base64url KID of device key>
X-Signature: <base64url Ed25519 signature of request body>
X-Timestamp: <ISO 8601 timestamp>
X-Nonce: <unique request nonce>
```

## Response format

### Success
```json
{
  "data": {
    "queryName": { ... }
  }
}
```

### Error
```json
{
  "data": null,
  "errors": [
    {
      "message": "Human-readable error",
      "locations": [{ "line": 1, "column": 1 }],
      "path": ["queryName"],
      "extensions": {
        "code": "ERROR_CODE"
      }
    }
  ]
}
```

## Error codes

| Code | HTTP Status | Meaning |
|------|-------------|---------|
| `VALIDATION_ERROR` | 400 | Input validation failed |
| `UNAUTHENTICATED` | 401 | Missing or invalid token |
| `FORBIDDEN` | 403 | Valid token, insufficient permissions |
| `NOT_FOUND` | 404 | Resource doesn't exist |
| `CONFLICT` | 409 | Duplicate resource (e.g., username, key already registered) |
| `RATE_LIMITED` | 429 | Too many requests |
| `INTERNAL_ERROR` | 500 | Unexpected server error |

Use `SCREAMING_SNAKE_CASE` for error codes. Domain-specific codes (e.g., `DUPLICATE_USERNAME`, `INVALID_SIGNATURE`) extend these base codes.

## Nullability rules

- IDs are never null
- Optional fields explicitly marked nullable in schema
- Empty arrays returned as `[]`, not `null`
- Timestamps in ISO 8601 format

## Breaking vs non-breaking changes

### Non-breaking (safe)
- Adding new fields to types
- Adding new queries/mutations
- Adding optional arguments
- Deprecating fields (with `@deprecated`)

### Breaking (requires versioning)
- Removing fields
- Changing field types
- Renaming fields
- Making optional fields required
- Changing argument types

## Health endpoints

| Endpoint | Purpose | Expected response |
|----------|---------|-------------------|
| `GET /health` | Liveness | `200 OK` |
| `GET /ready` | Readiness (DB connected) | `200 OK` or `503` |

## Rate limiting

**Not yet implemented.** Rate limiting is planned but no code exists. When built, it should follow the secure defaults policy (enabled with conservative limits by default).
