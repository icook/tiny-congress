# API Contracts

## GraphQL endpoint

**URL:** `/graphql` (POST)

**Headers:**
```
Content-Type: application/json
Authorization: Bearer <token>  # When authenticated
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
| `UNAUTHENTICATED` | 401 | Missing or invalid token |
| `FORBIDDEN` | 403 | Valid token, insufficient permissions |
| `NOT_FOUND` | 404 | Resource doesn't exist |
| `VALIDATION_ERROR` | 400 | Input validation failed |
| `INTERNAL_ERROR` | 500 | Unexpected server error |

## Pagination

Use cursor-based pagination for lists:

```graphql
query {
  items(first: 10, after: "cursor123") {
    edges {
      node { id, name }
      cursor
    }
    pageInfo {
      hasNextPage
      endCursor
    }
  }
}
```

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

- Default: 100 requests/minute per IP
- Authenticated: 1000 requests/minute per user
- Headers returned:
  - `X-RateLimit-Limit`
  - `X-RateLimit-Remaining`
  - `X-RateLimit-Reset`
