# FE-06 Endorsement editor

Goal: UI to create and revoke endorsements with magnitude/confidence sliders and optional context/evidence.

Deliverables
- Endorsement editor component `web/src/features/identity/components/EndorsementEditor.tsx` used on profile and standalone page.
- API hooks to POST `/endorsements` and `/endorsements/:id/revoke` with device-signed envelopes.
- Validation and feedback for signature and rate-limit errors.

Implementation plan (web)
1) Component: build form with select for subject/topic, sliders for magnitude (-1..1) and confidence (0..1), textarea for context, and URL input for evidence. Use Mantine components; show live preview of weighted mean contribution.

2) Signing: use FE-01 signer to wrap payload in envelope signed by current device key. Include `magnitude`/`confidence` as canonical numbers (keep consistent with backend expected format). Compute subject fields based on current view (e.g., endorsing profile user).

3) Submission: call `/endorsements` via API client. Handle 429 from BE-12 rate limiter and show tooltip countdown.

4) Revocation: add button next to each endorsement to POST revocation envelope. Refresh aggregates on success.

5) Tests: RTL tests mocking API, ensuring slider values are sent, signing helper invoked, and success updates UI. Test revocation path hides endorsement.

Verification
- `cd web && yarn test`.
- Manual: create and revoke endorsement against a test account; check aggregates in DB.
