# QR Endorsement Handshake — Design

**Date:** 2026-03-12
**Issue:** #613
**Spike:** `.plan/2026-03-11-m4-qr-handshake-spike-brief.md`, `.plan/2026-03-12-qr-handshake-spike-findings.md`

---

## Overview

Add a QR-based endorsement handshake to TinyCongress. Two authenticated users standing together: one generates a QR code (endorser), the other scans it (endorsee). A trust edge is created automatically, consuming one of the endorser's slots.

## Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Initiation model | Invite-first | Creating an invite = declaring intent to endorse. Maps to endorsement slot model (ADR-020). |
| Navigation | Dedicated `/endorse` route | Endorsing is an action, not a setting. Needs visibility in nav for demo. |
| Page layout | Single page with tabs | "Give Endorsement" and "Accept Endorsement" tabs. Minimal routing, everything discoverable. |
| QR scanning library | `nimiq/qr-scanner` | Actively maintained, iOS Safari workarounds, ~50KB. `html5-qrcode` is unmaintained. |
| QR generation library | `qrcode.react` | SVG rendering, crisp on all DPIs, React-native component. |
| QR content | Full URL: `https://{host}/endorse?invite={id}` | Works when scanned by native camera app. URL-based means fallback is just sharing a link. |
| QR display | 250x250 CSS px, ECL-M | Well above 114px minimum for 30cm scanning. ECL-M sufficient for screen display. |
| Fallback | Copy/share link + paste input | Covers WebView (camera blocked), no-camera devices, and remote endorsement. |
| Post-handshake UX | Stay on /endorse, show updated state | Confirmation message, updated slot count, new endorsement in list. |
| Auth requirement | Both parties must be authenticated | Simplifies demo — no signup-during-handshake flow. |
| Edge creation | Backend auto-endorses on invite accept | Signed invite envelope is stored proof of endorser intent. No second request needed. |

## Route & Feature Structure

**Route:** `/endorse` (auth-required, inside `authRequiredLayout`)

**Nav:** "Endorse" added between Rooms and About.

**Feature directory:**
```
web/src/features/endorsements/
├── api/
│   ├── client.ts      — signedFetchJson calls to trust endpoints
│   └── queries.ts     — TanStack Query hooks
├── components/
│   ├── GiveTab.tsx     — QR generation + share link
│   ├── AcceptTab.tsx   — QR scanner + paste link input
│   ├── SlotCounter.tsx — "2 of 3 endorsements used" progress bar
│   └── EndorsementList.tsx — active endorsements with revoke
├── index.ts            — barrel exports
└── types.ts            — shared types
```

**Page:** `web/src/pages/Endorse.page.tsx`

**Dependencies:** `qr-scanner`, `qrcode.react`

## API Surface

| Hook | Method | Endpoint | Purpose |
|------|--------|----------|---------|
| `useMyEndorsements()` | GET | `/me/endorsements` | Slot count + endorsement list |
| `useTrustBudget()` | GET | `/trust/budget` | Slot totals (endorsements_total, endorsements_used) |
| `useMyInvites()` | GET | `/trust/invites/mine` | Pending/accepted invites |
| `useCreateInvite()` | POST | `/trust/invites` | Mutation — generate invite |
| `useAcceptInvite(id)` | POST | `/trust/invites/{id}/accept` | Mutation — accept scanned/pasted invite |
| `useRevokeEndorsement()` | POST | `/trust/revoke` | Mutation — free up a slot |

**Cache invalidation:**
- After `createInvite`: invalidate `['my-invites']`
- After `acceptInvite`: invalidate `['my-endorsements']`, `['my-invites']`, `['verification-status']`
- After `revokeEndorsement`: invalidate `['my-endorsements']`, `['trust-budget']`

## Page Layout & UX Flow

**Page structure (top to bottom):**
1. **SlotCounter** — "2 of 3 endorsements used", progress bar (green → yellow → red)
2. **Tabs** — "Give Endorsement" | "Accept Endorsement"
3. **EndorsementList** — active endorsements with username, date, revoke button

### Give Endorsement Tab

1. User taps "Create Endorsement Invite"
2. `POST /trust/invites` with `delivery_method: "qr"`, `attestation: { method: "physical_qr" }`
3. Show QR code (250px SVG) encoding `https://{host}/endorse?invite={id}`
4. Below QR: "Share invite link" button (Web Share API → clipboard fallback)
5. Below that: invite expiry display
6. Slots full: button disabled, message "All endorsement slots used. Revoke one to endorse someone new."

### Accept Endorsement Tab

1. Two options: "Scan QR Code" button + "Or paste an invite link" text input
2. Scan QR: `qr-scanner` inline, rear camera, extract invite ID from URL
3. Paste link: extract invite ID from URL
4. Either → `POST /trust/invites/{id}/accept`
5. Success: "Endorsement received from {username}!"
6. Error: "This invite has expired or was already used" (404), "No connection" (network)

### URL Parameter Handling

`/endorse?invite={id}` auto-switches to Accept tab, pre-fills invite ID, prompts confirmation.

Login redirect preserves `?invite=` param through auth guard via TanStack Router search params.

## Backend Change

**`accept_invite_handler`** must be extended to auto-enqueue an endorsement after accepting:

1. `trust_repo.accept_invite(invite_id, auth.account_id)` — marks invite accepted (existing)
2. Read `endorser_id` and `attestation` from the accepted invite record
3. Call `trust_service.endorse(endorser_id, accepted_by, 1.0, attestation)` — enqueues trust edge creation

The stored signed invite envelope (`trust__invites.envelope`) is the endorser's authorization. No second request from the endorser needed.

**Slot check timing:** The `endorse` service method checks slot availability at enqueue time. If the endorser's slots are full when the invite is accepted, the endorsement fails. The invite is still marked accepted (single-use consumed). This is acceptable — the endorser shouldn't have created an invite with no slots available.

## Error Handling

| Scenario | Behavior |
|----------|----------|
| All slots used, create invite | Show warning (slot count visible), but allow creation. Endorsement may fail at accept time if slots still full. |
| Scan expired/used invite | 404 → "This invite has expired or was already used." |
| Camera permission denied | Hide scanner, surface paste-link input with helper text |
| Network error | "Connection error. Check your network and try again." |
| Self-endorsement | Backend rejects (`SelfAction`) → "You can't endorse yourself." |
| Duplicate endorsement | Backend rejects → "You've already endorsed this person." |
| Quota exceeded | Backend rejects (`QuotaExceeded`) → "Daily action limit reached. Try again tomorrow." |
| Navigate to `/endorse?invite={id}` while logged out | Auth guard → `/login?redirect=/endorse?invite={id}` → after login, land on accept flow |

## Components

**QRCodeDisplay (inside GiveTab):**
- `qrcode.react` `<QRCodeSVG>` — 250x250px, ECL-M, white background
- Content: `https://{host}/endorse?invite={invite_id}`

**QRScanner (inside AcceptTab):**
- `qr-scanner` instance on a `<video>` element
- Prefer rear camera, highlight scan region
- Auto-stop after successful scan
- On decode: validate URL pattern, extract invite ID

**SlotCounter:**
- Mantine `Progress` + text
- Color: green (0-1 used), yellow (2), red (3/full)

**EndorsementList:**
- Mantine `Stack` of `Card` components
- Each: endorsed username, date, revoke button
- Empty state: "No endorsements yet."
