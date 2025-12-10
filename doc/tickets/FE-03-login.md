# FE-03 Login

Goal: device-key login flow using challenge/response, persisting session tokens in the client.

Deliverables
- Login screen under `web/src/features/identity/screens/Login.tsx` with challenge + verify steps.
- API client wrappers for `/auth/challenge` and `/auth/verify`.
- Session state management (store token, account_id/device_id) and automatic injection into API calls.

Implementation plan (web)
1) UI/Router: add route `login` in `Router.tsx`. Provide fields for account_id + device_id (or username -> account lookup once backend supports). Show QR/clipboard instructions for device ID.

2) Flow:
   - Call `/auth/challenge` with `{ account_id, device_id }`; receive `{ challenge_id, nonce, expires_at }`.
   - Use FE-01 signer to canonicalize `{ challenge_id, nonce, account_id, device_id }` and sign with device key.
   - POST to `/auth/verify` with signature; store returned session token in `session.ts` (likely localStorage + memory) and set default Authorization header for API client.

3) UX: show countdown to expiry; disable verify button when challenge expired or missing local device key. Provide link to “Add device” flow if device key missing.

4) Tests: Vitest + RTL to mock API responses; assert challenge is fetched, sign function called, and token stored. Add test for expired nonce (backend returns 401) showing error banner.

Verification
- `cd web && yarn test`.
- Manual: run backend locally, login with a test account, and inspect session storage.
