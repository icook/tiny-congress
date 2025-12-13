# INF-02 Secrets and key handling

Goal: manage session signing keys and other secrets securely across environments, with rotation plan.

Deliverables
- Configuration for session token signing key (prefer KMS/secret manager; fallback env var in dev).
- Documented env vars and rotation steps.
- Validation that secrets are not committed to git.

Implementation plan
1) Session signing keys: use HS256 or EdDSA for session tokens. Read from `SESSION_SIGNING_KEY` env var in `service/src/identity/http/auth.rs`. For production, wire to cloud secret manager (placeholder if not available) and ensure deployment manifests reference the secret.

2) Root/device keys: never store server-side; only store public keys. Ensure logging scrubs any private key material. Add guardrails in handlers to reject payloads that accidentally include private keys.

3) Configuration docs: update `README.md` (service) with required env vars: `SESSION_SIGNING_KEY`, `RATE_LIMIT_SECRET` (if used), DB creds. Add sample `.env.example` excluding secrets. Emphasize `DATABASE_URL` must load `CREATE EXTENSION pgmq;` per AGENTS instructions.

4) Rotation: design a playbook in `doc/verify-artifacts.md` or new doc section describing how to rotate session signing key without dropping sessions (e.g., support key ID + dual verification window). Implement support for multiple signing keys (kid header) in session middleware.

5) Validation: add lint in CI to search for PEM blocks in repo (optional) or use `git-secrets` if available. Ensure Dockerfiles do not bake secrets.

Verification
- Manual: set `SESSION_SIGNING_KEY` locally, run login flow, and confirm tokens sign/verify.
- CI: add a small test that fails if `SESSION_SIGNING_KEY` is missing in test profile (except when using mock signer).
