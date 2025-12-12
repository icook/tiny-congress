/**
 * Test vectors matching backend crypto module tests
 * Source: service/src/identity/crypto/mod.rs
 */

import { hexToBytes } from './utils';

/**
 * Test secret key from backend: [0, 1, 2, ..., 31]
 */
export const TEST_SECRET_KEY = hexToBytes(
  '000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f'
);

/**
 * Expected public key derived from TEST_SECRET_KEY
 * Generated from ed25519.getPublicKey(TEST_SECRET_KEY) using @noble/curves
 */
export const TEST_PUBLIC_KEY = hexToBytes(
  '03a107bff3ce10be1d70dd18e74bc09967e4d6309ba50d5f1ddc8664125531b8'
);

/**
 * Expected kid for TEST_PUBLIC_KEY
 * Computed as: base64url(SHA256(TEST_PUBLIC_KEY))
 */
export const TEST_KID = 'Vkdap1RjR0wChd9dvyvKtz2mUTWIOem3dIGy6rEHcIw';

/**
 * Expected signature for test envelope from backend test:
 * sign_and_verify_matches_expected_signature
 *
 * Envelope:
 * {
 *   "v": 1,
 *   "payload_type": "Test",
 *   "payload": {"body": {"foo": "bar"}, "prev_hash": null},
 *   "signer": {"account_id": null, "device_id": null, "kid": TEST_KID}
 * }
 */
export const TEST_ENVELOPE_SIGNATURE =
  'hYIISBD5RFoDlp969r48FHviKhIjSfpR3K2aKKb3OAq7hffkI042G1mCvU3MD7AsGpFuzSeZOojtpIBU5gigCw';

/**
 * Test envelope payload matching backend
 */
export const TEST_ENVELOPE_PAYLOAD = {
  body: { foo: 'bar' },
  prev_hash: null,
};

/**
 * Canonical JSON bytes for test envelope (without signature)
 * Expected canonical form from JCS (JSON Canonicalization Scheme - RFC 8785)
 */
export const TEST_CANONICAL_BYTES = new TextEncoder().encode(
  `{"payload":{"body":{"foo":"bar"},"prev_hash":null},"payload_type":"Test","signer":{"account_id":null,"device_id":null,"kid":"${TEST_KID}"}}`
);
