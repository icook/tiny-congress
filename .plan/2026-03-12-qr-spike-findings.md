# QR Handshake Spike — Findings

**Date:** 2026-03-12
**PR:** #618
**Issue:** #613

## Browser Compatibility Matrix

| Browser | QR Generation | Camera Init | QR Decode | Notes |
|---------|:---:|:---:|:---:|-------|
| iPhone Safari (iOS 17+) | | | | |
| Chrome Android | | | | |
| iMessage in-app browser | | | | |
| WhatsApp in-app browser | | | | |
| Desktop Chrome | | | | |
| Desktop Safari | | | | |

## QR Code Sizing

- Minimum QR size for reliable scan at ~30cm: ___ px
- Tested content lengths: invite URL (~80 chars)
- Error correction level used: M (default)

## Latency Measurements

| Browser | Camera Init (ms) | Init → First Decode (ms) |
|---------|:-:|:-:|
| iPhone Safari | | |
| Chrome Android | | |
| Desktop Chrome | | |

## Library Assessment

### qrcode (v1.5.4)
- CDN size: ~50KB
- Canvas rendering:
- Notes:

### html5-qrcode (v2.3.8)
- CDN size: ~300KB
- Camera permission UX:
- Decode reliability:
- Notes:

## Backend Round-Trip

- Create invite:
- QR encode invite URL:
- Scan + decode:
- Accept invite:
- End-to-end latency:

## Blockers / Workarounds

- (none yet)

## Recommendation

TBD after testing.
