# FE-02 Signup

Goal: build signup flow that creates an account with root key + first device delegation, calling the backend endpoints.

Deliverables
- Signup screen under `web/src/features/identity/screens/Signup.tsx` wired into router.
- Form to capture username + device metadata, generate root/device keys (via FE-01), create delegation envelope, and POST to `/auth/signup`.
- UI feedback for success/error and storage of session token.

Implementation plan (web)
1) Routing/UI: add route `createRoute({ path: 'signup', component: Signup })` in `web/src/Router.tsx`. Use Mantine form components consistent with existing theme.

2) Flow:
   - On submit, call FE-01 key generation if keys not present.
   - Build DeviceDelegation payload (includes device_id, device_pubkey, metadata, ctime) and sign with root key using canonicalization helper.
   - Call backend `/auth/signup` (use new API client in `web/src/features/identity/api/client.ts`) with username/root_pubkey/device_pubkey/metadata/delegation_envelope.
   - On success, persist returned account_id/device_id and session token (if backend returns one) in a session store (`web/src/features/identity/state/session.ts`).

3) Error handling: surface duplicate username, bad signature errors as inline messages. Add spinner/disabled submit while request in-flight.

4) Tests: add `Signup.test.tsx` with React Testing Library using mocked API client; assert envelope creation happens and success path navigates to `/dashboard` or `/account`. Include validation errors.

Verification
- `cd web && yarn test`.
- Manual: run `yarn dev`, visit `/signup`, create account, verify DB row exists and device stored.
