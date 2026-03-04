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

## REST endpoint reference

For full request/response schemas and error codes, see [domain-model.md](../domain-model.md).

### Identity (`/auth/*`)

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| POST | `/auth/signup` | No | Create account with root key, device key, and backup |
| GET | `/auth/backup/{username}` | No | Retrieve encrypted backup envelope (anti-enumeration) |
| POST | `/auth/login` | No | Authenticate and register new device key |
| GET | `/auth/devices` | Yes | List all device keys for account |
| POST | `/auth/devices` | Yes | Add a device key |
| DELETE | `/auth/devices/{kid}` | Yes | Revoke a device key |
| PATCH | `/auth/devices/{kid}` | Yes | Rename a device key |

### Reputation (`/me/*`, `/endorsements/*`, `/verifiers/*`)

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/me/endorsements` | Yes | List caller's endorsements |
| GET | `/endorsements/check` | No | Check endorsement (`?subject_id=&topic=`) |
| POST | `/verifiers/endorsements` | Yes (verifier) | Create endorsement for a user |
| GET | `/auth/idme/authorize` | Yes | Get ID.me OAuth redirect URL |
| GET | `/auth/idme/callback` | No | ID.me OAuth callback (browser redirect) |

### Rooms (`/rooms/*`)

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/rooms` | No | List open rooms |
| POST | `/rooms` | Yes | Create a room |
| GET | `/rooms/{room_id}` | No | Get room details |
| GET | `/rooms/{room_id}/polls` | No | List polls in room |
| POST | `/rooms/{room_id}/polls` | Yes | Create a poll |
| GET | `/rooms/{room_id}/polls/{poll_id}` | No | Get poll with dimensions |
| POST | `/rooms/{room_id}/polls/{poll_id}/status` | Yes | Update poll status |
| POST | `/rooms/{room_id}/polls/{poll_id}/dimensions` | Yes | Add dimension to poll |
| POST | `/rooms/{room_id}/polls/{poll_id}/vote` | Yes | Cast votes (eligibility-gated) |
| GET | `/rooms/{room_id}/polls/{poll_id}/results` | No | Get aggregate results |
| GET | `/rooms/{room_id}/polls/{poll_id}/my-votes` | Yes | Get caller's votes |

### Other

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/api/v1/build-info` | No | Build version, git SHA, timestamp |
| GET | `/health` | No | Liveness probe (`200 OK`) |
| GET | `/ready` | No | Readiness probe (`200 OK` or `503`) |
| POST | `/graphql` | No | GraphQL endpoint |

## Rate limiting

**Not yet implemented.** Rate limiting is planned but no code exists. When built, it should follow the secure defaults policy (enabled with conservative limits by default).
