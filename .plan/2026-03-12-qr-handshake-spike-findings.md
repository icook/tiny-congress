# M4 QR Handshake — Spike Findings

**Date:** 2026-03-12
**Spike brief:** `.plan/2026-03-11-m4-qr-handshake-spike-brief.md`
**Test page:** `web/public/spike-qr.html` (access via `just dev-frontend` → `/spike-qr.html`)

---

## Recommendation

Use **`nimiq/qr-scanner`** (npm: `qr-scanner`) instead of `html5-qrcode`. Use **`qrcode.react`** or the `qrcode` npm package for generation.

---

## Library Validation

### html5-qrcode (v2.3.8)

| Question | Finding |
|----------|---------|
| Mobile Safari (iOS 17+) | Functional but unreliable. Multiple open issues (#512, #618, #136) — scan failures related to frame rate and WebKit media pipeline. Falls back to JS decoder (no BarcodeDetector on iOS), which is slow and CPU-heavy. |
| Chrome Android | Works reliably. Can use BarcodeDetector API for hardware-accelerated decoding (Chrome 83+). Best-supported platform. |
| WebView / in-app browsers | **Broken.** iOS WKWebView (iMessage, WhatsApp links) blocks camera access. Android WebView similarly restricted. Issue #544, #856 document failures. |
| Camera permission UX | Standard browser prompt. Works correctly on full browsers, fails silently or errors in WebViews. |
| Permission denial recovery | No built-in recovery — stays in error state. App must handle retry UX. |
| Startup latency | 2–5 seconds typical (permission prompt + stream init + first decode at 2fps default). |
| Maintenance status | **Unmaintained.** Last release April 2023. PRs not being merged. Uses abandoned zxing-js port. |

**Verdict: Do not use.** Unmaintained, known iOS issues, slow scanning.

### nimiq/qr-scanner (recommended alternative)

| Property | Detail |
|----------|--------|
| Maintenance | Actively maintained |
| Bundle size | ~50 KB |
| iOS Safari | Has WebKit-specific workarounds built in |
| Decode method | Web worker (non-blocking), uses jsQR |
| API | Simpler than html5-qrcode |

### Native BarcodeDetector API

| Browser | Support |
|---------|---------|
| Chrome/Edge (desktop + Android) | Yes (Chrome 83+) |
| Firefox | No |
| Safari (macOS + iOS) | No (experimental flag broken in iOS 18+) |
| Samsung Internet | Yes |

**Verdict: Cannot replace a library.** iOS has no support, which rules it out as a standalone solution. Can be used as an optional fast path on Android.

---

## QR Code Generation

| Question | Finding |
|----------|---------|
| SVG rendering on high-DPI | `qrcode.react` SVG renders pixel-perfect on all DPIs. Canvas also works. |
| Minimum scannable size at 30cm | **3 cm / ~114 CSS px** (10:1 rule: distance ÷ 10). Recommend **4–5 cm / ~160–190 px** for comfort. |
| URL length (~70 chars) + ECL-M | Fits in QR Version 3 (29×29 modules). ECL-M (15% recovery) is appropriate for screen display. ECL-H only needed for printed codes. |
| URL vs raw invite ID | **Encode a URL.** If the QR is scanned by a native camera app (not our scanner), the URL opens the browser and lands on the handshake page. Raw ID requires our app to already be open. |

**Recommended QR size for the app: 250×250 CSS px** (well above minimum, comfortable at arm's length).

---

## End-to-End Flow

| Question | Finding |
|----------|---------|
| Complete in < 5 seconds? | Likely yes on Android (BarcodeDetector fast path). iOS may take 3–5 seconds due to JS-only decoder. Needs real-device validation with spike page. |
| No network on scanner? | Scan succeeds (QR decode is local), accept API call fails with network error. Need clear "no connection" UI. |
| Expired invite? | Backend returns 404 (expired and not-found are indistinguishable). Need UX copy: "This invite has expired or was already used." |
| Double scan? | Backend returns 404 (same as expired). Single-use enforcement works. Error message should cover both cases. |

---

## Fallback Strategy

| Scenario | Fallback |
|----------|----------|
| Camera scanning fails | "Copy invite link" button + "Paste invite link" input field. Already works with URL-based QR. |
| In-app browser (WebView) | Detect WebView and show "Open in Safari/Chrome" prompt with the link. Do NOT attempt camera access. |
| Share via messaging | Deep link sharing (tap to share via system share sheet). The URL is the invite — it works whether scanned or tapped. |

---

## WebView Detection

In-app browsers are the biggest risk. Recommended detection heuristic:

```typescript
function isInAppBrowser(): boolean {
  const ua = navigator.userAgent;
  return /FBAN|FBAV|Instagram|WhatsApp|Line|Snapchat|Twitter|LinkedIn/i.test(ua)
    || (!/Safari/i.test(ua) && /iPhone|iPad/i.test(ua)); // iOS non-Safari
}
```

When detected, show a banner: **"For the best experience, open this link in Safari"** with a copy-link button.

---

## Decision

Per the spike brief decision criteria:

| Outcome | Decision |
|---------|----------|
| `html5-qrcode` works on iOS + Android | **No** — works on Android, unreliable on iOS, unmaintained |
| `html5-qrcode` fails, `BarcodeDetector` works | BarcodeDetector not available on iOS |
| Camera scanning unreliable on mobile | Partially — unreliable in WebViews, workable in full browsers |

**Chosen path:** Use `nimiq/qr-scanner` (actively maintained, iOS workarounds) with URL-based QR codes. Include copy-link fallback for WebView contexts. Proceed with M4 implementation plan using this stack.

---

## Implementation Recommendations for M4

1. **Dependencies to add:** `qr-scanner` (scanning), `qrcode.react` (generation)
2. **QR content:** Full URL (`https://host/handshake/{invite-id}`)
3. **QR display:** 250×250px, ECL-M, white background with sufficient quiet zone
4. **Scanner config:** Prefer rear camera, 10fps scan rate, 250×250 scan region
5. **WebView handling:** Detect and redirect to system browser before attempting camera
6. **Fallback UI:** Always show "Share link" and "Paste invite link" alongside QR
7. **Error states:** "Invite expired or already used" (covers 404), "No connection" (covers network errors)

---

## Open Questions (for real-device testing)

These require the spike test page (`/spike-qr.html`) on physical devices:

- [ ] Actual latency with `qr-scanner` on iOS Safari vs html5-qrcode
- [ ] Does `qr-scanner` handle iOS camera permission smoothly?
- [ ] Scan reliability at 250px QR size from 30cm with various phone cameras
- [ ] Performance on older devices (iPhone 11-era, mid-range Android)

Update this document with real-device results when available.
