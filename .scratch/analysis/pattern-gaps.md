# Pattern Analysis: Documentation Gaps

Analysis of patterns in `feature/phase1-identity-tickets` vs existing docs.

## Identified Gaps

### 1. Feature-Based Frontend Organization

**Documented** (`directory-conventions.md`):
```
web/src/components/    # React components
web/src/pages/         # Route pages
```

**Actual**:
```
web/src/features/identity/
├── api/client.ts      # API client + types
├── keys/              # Crypto module (barrel export)
├── screens/           # Route pages for feature
├── state/session.ts   # State management
```

**Gap**: No guidance on `features/` structure, when to use it, internal conventions.

---

### 2. Rust Domain Module Structure

**Documented**: Partially in `rust-coding-standards.md`

**Actual**:
```
service/src/identity/
├── crypto/     # Cryptographic operations
├── http/       # HTTP handlers + router composition
├── repo/       # Data persistence
├── policy/     # Authorization engine
├── abuse/      # Rate limiting + audit
└── sigchain/   # Sigchain verification
```

**Gap**: Standard subdirectory pattern not documented.

---

### 3. API Client Pattern

**Documented**: None

**Actual** (`web/src/features/identity/api/client.ts`):
- `ApiError` class
- Generic `fetchJson<T>()` helper
- Types + functions grouped by concern
- Auth header pattern

**Gap**: No standard for frontend API code organization.

---

### 4. Signed Envelope Spec

**Documented**: None (critical security pattern)

**Actual**: Cross-cutting pattern in both BE and FE:
```typescript
interface SignedEnvelope {
  v: number;
  payload_type: string;
  payload: unknown;
  signer: { account_id?, device_id?, kid };
  sig: string;  // base64url Ed25519
}
```

**Gap**: No specification for envelope structure, canonicalization (RFC 8785), signature algorithm.

---

### 5. Barrel Export Pattern

**Documented**: None

**Actual** (`web/src/features/identity/keys/index.ts`):
```typescript
export type { KeyPair, StoredKey, ... } from './types';
export { deriveKid } from './kid';
export { signEnvelope } from './signer';
```

**Gap**: When/how to use barrel exports not documented.

---

### 6. Error Handling Mismatch

**Documented** (`rust-coding-standards.md`): Use `thiserror` enums

**Actual** (`http/accounts.rs`): Uses `(StatusCode, String)` tuples

**Gap**: Either migrate code or document as "legacy pattern with migration path".

---

## Recommended Actions

1. **Update `directory-conventions.md`**:
   - Add `features/` pattern
   - Document Rust domain module structure

2. **Create `frontend-patterns.md`**:
   - API client structure
   - Barrel exports
   - State management

3. **Create `signed-envelope-spec.md`**:
   - Envelope JSON structure
   - Canonicalization requirement
   - Signature algorithm

4. **Update `rust-coding-standards.md`**:
   - Note current `(StatusCode, String)` as legacy
   - Add migration guidance
