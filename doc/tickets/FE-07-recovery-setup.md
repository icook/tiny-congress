# FE-07 Recovery setup

Goal: UI to configure recovery helpers and threshold, display approvals, and trigger root rotation flow.

Deliverables
- Recovery setup screen `web/src/features/identity/screens/Recovery.tsx`.
- Forms to add helpers, set threshold, and submit root-signed policy.
- UI to display pending approvals and trigger rotate-root once threshold met.

Implementation plan (web)
1) Data fetch: API client calls to `/me/recovery_policy` and `/recovery/approve`/`/recovery/rotate_root`. Cache policy and approvals.

2) Policy creation:
   - Use FE-01 root key to sign policy payload (helpers list, threshold, ctime) into envelope.
   - POST to `/me/recovery_policy` and render active policy with status badges per helper.

3) Approvals view: show helper list with status (pending/approved). For helpers viewing their queue, provide button to approve using device key signature and POST `/recovery/approve`.

4) Root rotation: when approvals >= threshold, show CTA to rotate root. Prompt for new root key generation (or import). Sign rotation envelope if needed or call backend endpoint that validates approvals. Warn that devices will be invalidated until re-delegated.

5) Tests: RTL tests covering policy creation, approval submission, and rotation button disabled/enabled logic. Mock API responses.

Verification
- `cd web && yarn test`.
- Manual: create policy, submit helper approval from another browser profile, rotate root, verify device sessions invalidated.
