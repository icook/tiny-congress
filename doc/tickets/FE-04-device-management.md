# FE-04 Device management

Goal: UI for listing devices, adding via QR handoff, and revoking devices with root key confirmation.

Deliverables
- Devices page under `web/src/features/identity/screens/Devices.tsx` with list + actions.
- API client methods for `/me/devices`, `/me/devices/add`, `/me/devices/:id/revoke`.
- QR add flow that generates payload for an existing device to sign.

Implementation plan (web)
1) Data fetching: use TanStack Router loaders or custom hook `useDevices()` hitting `/me/devices`. Display name/type/created_at/last_seen/revoked_at.

2) Add device:
   - Show QR code containing `{ account_id, new_device_pubkey, metadata, nonce }` generated locally.
   - Provide instructions for existing device to scan and submit delegation. After backend confirmation, refresh list.

3) Revoke device:
   - Trigger root-key prompt (unlock via FE-01). Build revocation envelope, sign with root key, POST to `/me/devices/:id/revoke`.
   - Update UI to show revoked status, disable login badge.

4) UX: align with Mantine components; show security posture summary snippet from BE-08 on this screen (active device count, last seen).

5) Tests: RTL tests mocking API; ensure add/revoke buttons call correct client methods and state updates. Snapshot test for revoked badge styling.

Verification
- `cd web && yarn test`.
- Manual: add a device via QR between two tabs, then revoke and confirm login fails.
