// Stub for @/wasm/tc-crypto/tc_crypto.js used in unit tests.
// The real file is built by `just build-wasm` and not committed.
export default async function init() {}
export const derive_kid = () => 'kid-stub';
export const encode_base64url = () => '';
export const decode_base64url = () => new Uint8Array(0);
