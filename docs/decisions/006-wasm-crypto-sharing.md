# ADR-006: Share Crypto Code via WASM

## Status
Accepted

## Context
The TinyCongress backend (Rust) and frontend (TypeScript) both need to perform identical cryptographic operations for the identity system:
- Key ID (KID) derivation: `base64url(SHA-256(pubkey)[0:16])`
- Base64url encoding/decoding (RFC 4648)
- (Future) Ed25519 signing and verification
- (Future) RFC 8785 JSON canonicalization

Duplicating crypto logic across languages is error-prone. We previously had a bug where the backend used a 32-byte hash while the frontend used 16-byte truncation, causing KID mismatches.

## Decision
Create a shared `tc-crypto` Rust crate that:
1. **Backend uses natively** as a Cargo workspace dependency
2. **Frontend uses via WASM** compiled with `wasm-pack`

The crate is located at `crates/tc-crypto/` and provides:
- `derive_kid(public_key: &[u8]) -> String`
- `encode_base64url(bytes: &[u8]) -> String`
- `decode_base64url(encoded: &str) -> Result<Vec<u8>, Error>`

The frontend loads WASM via a React `CryptoProvider` that initializes the module before any components render.

## Consequences

### Positive
- **Single source of truth**: One implementation for both platforms
- **Cryptographic consistency**: Backend and frontend guaranteed to produce identical output
- **Type safety**: Rust catches errors at compile time
- **Testable**: Cross-language compatibility tests verify both sides match

### Negative
- **Build complexity**: Requires `wasm-pack` tooling and WASM build step
- **Async loading**: Frontend must wait for WASM module to load before crypto is available
- **Browser support**: WASM requires modern browsers (96%+ support, acceptable tradeoff)

### Neutral
- WASM bundle size (~40KB gzipped) is acceptable and loads asynchronously
- Key generation still uses `@noble/curves` in frontend (WASM `getrandom` requires additional setup)

## Alternatives considered

### Alternative A: Keep separate implementations
- Maintain identical code in both Rust and TypeScript
- **Rejected**: High risk of implementation drift, already caused bugs

### Alternative B: Backend-only crypto (API calls)
- Frontend sends data to backend for all crypto operations
- **Rejected**: Adds latency, requires network for local-first key generation

### Alternative C: Pure TypeScript shared library
- Write crypto in TypeScript, import in both (Rust via wasm32-unknown-unknown)
- **Rejected**: Rust is better suited for crypto (type safety, no runtime errors)

## References
- Issue #118: Share crypto code between Rust backend and TypeScript frontend via WASM
- [wasm-pack](https://rustwasm.github.io/wasm-pack/)
- [wasm-bindgen](https://rustwasm.github.io/wasm-bindgen/)
