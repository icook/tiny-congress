# M4 QR Handshake — Spike Brief

**Date:** 2026-03-11
**Purpose:** Define what needs to be validated before committing to a QR handshake implementation approach. This is a mobile-first interaction with real-device constraints that can't be planned from a desk.

---

## Why a Spike

The QR handshake is the demo's signature interaction — two people standing together, one generates a code, the other scans it, trust score updates, room unlocks. But:

1. **Camera APIs vary wildly across mobile browsers.** Mobile Safari, Chrome on Android, and in-app browsers (shared links opened from iMessage, WhatsApp) all behave differently.
2. **Library maturity is unknown.** `html5-qrcode` is the obvious choice, but its mobile Safari support and permission handling need real-device testing.
3. **The physical ritual has UX requirements that can't be tested headlessly.** QR code size, scanning distance, camera focus time, lighting conditions.
4. **The TRD describes JWT-signed QR codes.** The backend uses invite IDs with expiry + single-use. These are equivalent security-wise, but the spike should confirm the simpler approach (URL with invite ID) works end-to-end.

## Questions to Answer

### Library Validation
- [ ] Does `html5-qrcode` work on Mobile Safari (iOS 17+)?
- [ ] Does it work on Chrome for Android (latest)?
- [ ] Does it work in WebView contexts (iMessage link preview → Safari, WhatsApp in-app browser)?
- [ ] What's the camera permission UX? Does it use the standard browser permission prompt?
- [ ] How does it handle permission denial? Recovery path?
- [ ] What's the startup latency (camera init → first successful scan)?
- [ ] Alternative: Does the native `BarcodeDetector` API cover enough browsers to skip the library entirely?

### QR Code Generation
- [ ] `qrcode.react` renders SVG — is it crisp enough on high-DPI phone screens?
- [ ] What's the minimum QR code size (in CSS px) that's reliably scannable from 30cm on a phone camera?
- [ ] URL length: `https://host/handshake/{uuid}` ≈ 70 chars. QR error correction level M sufficient?
- [ ] Should the QR encode a URL (opens browser) or a raw invite ID (requires the app to be open)?

### End-to-End Flow
- [ ] Generator phone shows QR → Scanner phone scans → accept endpoint called → both phones show confirmation. Does this complete in < 5 seconds?
- [ ] What happens if the scanner's phone has no network? (Error before or after scan?)
- [ ] What happens if the invite expired between generation and scan? (Clear error?)
- [ ] What happens if someone scans the same code twice? (Backend enforces single-use — verify error message is clear)

### Fallback
- [ ] If camera scanning fails, is "copy invite link" + "paste invite link" a viable fallback?
- [ ] Should the QR code also be shareable as a deep link (tap to share via iMessage/WhatsApp)?

## Spike Approach

### Phase 1: Isolated HTML test page (30 min)
Build a standalone HTML page (not in the React app) that:
1. Has a button to generate a QR code containing a URL
2. Has a button to start camera scanning
3. Displays the decoded text on successful scan

Test on: iPhone Safari, Android Chrome, one in-app browser.

### Phase 2: Backend round-trip (30 min)
Wire the test page to:
1. Call `POST /trust/invites` (needs auth — use a hardcoded device key for the spike)
2. Generate QR from the invite URL
3. On scan, call `POST /trust/invites/{id}/accept` (needs auth on scanner's side too)
4. Confirm both sides get confirmation

### Phase 3: Findings doc (15 min)
Document:
- Which library/API to use
- Minimum QR size
- Mobile browser compatibility matrix
- Latency measurements
- Any blockers or required workarounds

## Decision Criteria

| Outcome | Next Step |
|---------|-----------|
| `html5-qrcode` works on iOS + Android | Use it, proceed with M4 plan |
| `html5-qrcode` fails on iOS but `BarcodeDetector` works | Use native API with polyfill |
| Camera scanning unreliable on mobile | Fall back to share-link flow (no camera) |
| Camera scanning works but too slow (>10s) | Investigate native camera intent (`input type="file" capture`) |

## Dependencies

- Backend must be running with trust endpoints active
- Need two physical phones (or one phone + one laptop with camera)
- Need the app deployed somewhere both devices can reach (local dev with ngrok, or staging)
- Need a seeded account with a device key for auth

## Out of Scope for Spike

- React component design (that's M4 planning, after spike validates the approach)
- Visual design of the QR screen
- Trust score update polling/display
- Invite management UI
