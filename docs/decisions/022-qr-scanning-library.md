# ADR-022: QR Scanning Library

## Status
Accepted

## Context
The endorsement handshake (M4) requires scanning QR codes on mobile devices. Camera APIs vary across mobile browsers, and in-app browsers (iMessage, WhatsApp) block camera access entirely. The library choice directly affects whether the demo's signature interaction works on iPhones — the primary device of the target audience.

## Decision
Use **`nimiq/qr-scanner`** (npm: `qr-scanner`) for scanning and **`qrcode.react`** for generation.

QR codes encode full URLs (`https://host/endorse?invite={id}`) at 250×250 CSS px, ECL-M. This ensures codes scanned by a native camera app (outside our scanner) open the browser and land on the correct page.

## Consequences

### Positive
- Actively maintained with iOS Safari workarounds built in
- Web worker-based decoding (non-blocking)
- ~50 KB bundle size
- Simpler API than html5-qrcode

### Negative
- Uses JS-only decoder (jsQR) — no hardware acceleration on any platform
- iOS scanning may take 3–5 seconds vs near-instant on Android with BarcodeDetector

### Neutral
- Copy-link fallback is always available for WebView contexts where camera is blocked

## Alternatives considered

### html5-qrcode (v2.3.8)
- Most popular QR scanning library for web
- **Rejected:** Unmaintained (last release April 2023), unreliable on iOS Safari (multiple open issues), uses abandoned zxing-js port

### Native BarcodeDetector API
- Hardware-accelerated, zero-dependency
- **Rejected:** No support on iOS Safari or Firefox. Cannot be used as standalone solution. Could be an optional fast path on Android in the future.

## References
- Spike findings: `.plan/2026-03-12-qr-handshake-spike-findings.md` (graduated into this ADR)
- Spike test page: `web/public/spike-qr.html`
