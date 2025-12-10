# BE-11 AuthZ engine skeleton

Goal: provide a reusable authorization entry point with attribute context builder and policy evaluator that Phase 1 endpoints can call.

Deliverables
- Attribute builder for static/dynamic attributes (roles, tier, reputation_score, security posture).
- Policy representation (JSON AST) and evaluator function.
- Hardcoded policies for Phase 1 actions (endorsements, device ops, recovery).

Implementation plan (service)
1) Module: `service/src/identity/policy/` with:
   - `attributes.rs`: functions to fetch account role/tier, computed reputation_score (from BE-08), and security_posture_score. Use existing DB pool to build `AttributeContext { account_id, roles, tier, reputation_score, security_posture_score }`.
   - `ast.rs` defining policy JSON AST (operators: and/or/not, eq, in, gte/lte).
   - `evaluator.rs` with `authorize(subject, action, resource, context) -> bool` applying AST or hardcoded defaults.

2) Storage: add `policies` table if needed (policy_id, name, jsonb) or keep in-memory map for Phase 1. For now, store a small set inline and allow overrides via env flag.

3) Integration: wrap endpoints with a helper `require(action, resource)` that pulls attributes for the caller (from session) and applies `authorize`. Examples: endorsement creation requires not revoked device; device revoke requires owner + root possession; recovery rotate requires threshold satisfied (handled earlier) plus posture >= minimal.

4) Tests: unit tests for AST evaluation (truth tables), attribute builder (mock DB rows), and an integration test ensuring an account with low posture fails a policy check when expected.

Verification
- `cd service && cargo test identity_policy`.
- `skaffold test -p ci` to keep policies evaluated in containerized suites.
