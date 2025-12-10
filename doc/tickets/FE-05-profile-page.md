# FE-05 Profile page

Goal: surface tier badges, security posture summary, and endorsements grouped by topic.

Deliverables
- Profile screen at `web/src/features/identity/screens/Profile.tsx` (routed via `/account` route).
- Components for tier badge, posture summary, and endorsement topic bins.
- API client calls for `GET /users/:id`, `/users/:id/security_posture`, `/users/:id/endorsements`, `/users/:id/reputation`.

Implementation plan (web)
1) Data loading: use Router loader to fetch profile + posture + reputation + endorsements in parallel (or use React Query if preferred). Cache results per account_id.

2) UI elements:
   - Tier badge mapping tiers to colors (anonymous/verified/bonded/vouched). Include verification_state chip.
   - Posture summary card showing device count, MFA factors usage, coarse label (weak/ok/strong) from BE-08.
   - Endorsement bins: group endorsements by topic, display counts and weighted mean; include filter by topic registry.
   - Reputation score pill (v0 heuristic) with tooltip describing components.

3) Accessibility: use semantic headings; ensure charts (if any) have text backups. Keep responsive layout (cards stack on mobile).

4) Tests: RTL tests with mocked API responses covering multiple tiers/postures. Verify grouping logic and fallback when no endorsements.

Verification
- `cd web && yarn test`.
- Manual: load profile for seeded account; cross-check numbers against DB aggregates.
