# ADR-007: Adopt ZIP215 semantics for Ed25519 verification

**Date:** 2025-12-17
**Status:** Draft
**Decision owners:** TinyCongress core team

## Context

TinyCongress relies on Ed25519 signatures for identity, endorsements, and protocol actions. Verification correctness must be stable across implementations and environments (Rust backend, browser UI, potential third-party nodes).

Ed25519 has known edge cases where different libraries disagree on signature validity, especially around:

* Non-canonical encodings.
* Small-order points.
* Cofactor-related behavior.

Divergent verification rules can cause consensus splits, replay acceptance mismatches, or "valid in one place, invalid in another" failures.

## Decision

We adopt **ZIP215** semantics as the canonical rule set for Ed25519 signature verification.

* All consensus-critical verification in TinyCongress MUST follow ZIP215.
* The Rust backend uses a ZIP215-compliant verifier (`ed25519-consensus` crate).
* Any non-Rust environment (UI, third-party tooling) must either:

  * Delegate verification to Rust (e.g., via WASM), or
  * Prove equivalence to ZIP215 via test vectors.

## Rationale

* ZIP215 is explicitly designed to **eliminate cross-implementation divergence** in Ed25519 verification.
* It defines acceptance rules that are stable even in adversarial or heterogeneous environments.
* It avoids historical footguns where "stricter" or "looser" interpretations silently disagree.
* This matches our long-term goal of open participation and third-party interoperability.

## Consequences

**Positive**

* Eliminates an entire class of consensus and interoperability bugs.
* Makes verification behavior explicit and documented.
* Supports future decentralization and third-party nodes safely.

**Negative**

* ZIP215 accepts some signatures that older "strict" verifiers may reject.
* Some popular Ed25519 libraries do not default to ZIP215 semantics and must be wrapped or replaced.
* Slightly more conceptual overhead for contributors unfamiliar with Ed25519 edge cases.
* Browser verification cannot use WebCrypto `verify()` — must always delegate to WASM.

## Alternatives considered

1. **RFC 8032-style strict verification only.**
   Rejected due to known divergence between implementations and lack of consensus safety guarantees.
2. **Library-default verification behavior.**
   Rejected because defaults vary and change over time.
3. **Ad-hoc "stricter than strict" rules.**
   Rejected as non-standard and likely to cause interoperability failures.
4. **Hybrid approach (WASM for consensus, WebCrypto for UI hints).**
   Rejected due to risk of UI showing "valid" when consensus would reject, and maintenance burden of two paths.

## Implementation notes

* **Signing is not affected:** ZIP215 concerns verification only. Signing via WebCrypto Ed25519 produces RFC 8032-compliant signatures, which are valid under ZIP215.
* **Browser verification must use WASM:** WebCrypto `verify()` does not implement ZIP215. All verification in the browser MUST go through the Rust/WASM module.
* Use `ed25519-consensus` crate (not `ed25519-dalek`) for ZIP215-compliant verification.
* Canonical test vectors (valid and invalid) are generated from Rust and run in CI across:

  * Backend verification.
  * Browser verification via WASM.
* Documentation explicitly states: "Signature validity is defined by ZIP215, not by library defaults."

## Related decisions

* [ADR-006: WebCrypto key recovery](006-webcrypto-key-recovery.md) — Signing uses WebCrypto; verification uses WASM per this ADR.
* [Signed Envelope Spec](../interfaces/signed-envelope-spec.md) — Envelope verification must use ZIP215.

## Follow-ups

* Publish a short "Why ZIP215" doc for contributors.
* Add fuzz and regression tests for malformed keys and signatures.
* Update signed-envelope-spec.md to reference ZIP215 requirement.
* Generate ZIP215-specific test vectors (small-order points, non-canonical S values).

## References

* [ZIP215 Specification](https://zips.z.cash/zip-0215)
* [ed25519-consensus crate](https://crates.io/crates/ed25519-consensus)
* [Taming the many EdDSAs](https://eprint.iacr.org/2020/1244) — Academic paper on Ed25519 verification variants
