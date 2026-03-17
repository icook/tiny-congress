# ADR-008: Account-Based Verifiers with Config Bootstrap

## Status
Accepted

## Context
TinyCongress needs a way for external identity verifiers (e.g., ID.me) to endorse user accounts for voting eligibility. The initial implementation used a dedicated `verifier_accounts` table with API-key authentication — verifiers were a separate entity class with their own auth mechanism.

This created several problems:
- Two authentication systems to maintain (device-key signing for users, Bearer tokens for verifiers)
- Verifier accounts couldn't be managed through the same identity infrastructure as regular accounts
- The API-key approach didn't align with TinyCongress's trust model where all authentication is based on Ed25519 key pairs
- No way for the sim worker to act as a verifier without a separate credential type

## Decision
Verifiers are regular accounts authenticated via the same device-key signing protocol as all other users. A verifier is simply an account that holds an `authorized_verifier` endorsement (with `issuer_id = NULL` for platform-bootstrapped verifiers).

**Bootstrap mechanism:** The API server reads `TC_VERIFIERS` (a JSON array of `{name, public_key}` entries) at startup and idempotently creates accounts + genesis endorsements. This runs on every startup with `ON CONFLICT DO NOTHING` semantics.

**Authentication flow:** Verifiers sign requests to `POST /verifiers/endorsements` using the same `X-Device-Kid` / `X-Signature` / `X-Timestamp` / `X-Nonce` headers as any authenticated user. The endpoint checks that the caller has the `authorized_verifier` endorsement before allowing the operation.

**Login:** Since bootstrap only creates the account with a root key (no device key), the verifier must call `POST /auth/login` to register a device key before making authenticated requests. The login certificate is `root_key.sign(device_pubkey || timestamp_le_i64_bytes)`.

## Consequences

### Positive
- Single authentication system — device-key signing for everything
- Verifiers are first-class accounts that can be audited, revoked, and managed like any other account
- The endorsement trail is clear: `issuer_id` on endorsements traces back to a real account
- Genesis endorsements (NULL issuer) are distinguishable from earned endorsements
- The sim worker can act as a verifier using the same `SimAccount` identity infrastructure

### Negative
- Bootstrap config must be coordinated between the API server (`TC_VERIFIERS`) and any client that needs to act as that verifier (e.g., the sim worker must derive the same key pair)
- Verifier accounts consume a row in the accounts table (negligible cost, but they're mixed in with real users)

### Neutral
- The `verifier_accounts` table and API-key columns were removed in migration 10
- External verifiers (like ID.me) still use their own OAuth flow for identity verification but create endorsements through the same `POST /verifiers/endorsements` endpoint

## Alternatives considered

### API-key authentication (original approach)
- Verifiers authenticated with `Authorization: Bearer <api-key>`
- Rejected: violated the principle of a single auth mechanism, required maintaining a parallel credential system, and didn't produce auditable issuer trails on endorsements

### Service-to-service tokens (JWT/mTLS)
- Machine-to-machine auth using short-lived tokens or mutual TLS
- Rejected: over-engineered for the current scale, adds infrastructure complexity (token issuer, certificate management), and still requires a separate auth path

## References
- [ADR-017: Two-layer trust architecture](017-two-layer-trust-architecture.md) — verifier accounts are platform trust infrastructure; the exemption from slot limits is defined there
- PR #381, #384: Generalized verifier API implementation
- `service/src/reputation/bootstrap.rs`: Bootstrap logic
- `service/src/reputation/http/mod.rs`: Endorsement endpoint with `AuthenticatedDevice`
- `service/src/config.rs`: `VerifierConfig` struct (`TC_VERIFIERS` env var)
