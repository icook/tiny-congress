# Sidebar User Menu — Design

**Date:** 2026-03-17

## Problem

The logged-in user menu lives in the top-right header as a dropdown (Settings, Logout). Trust and Endorse are separate links in the sidebar nav. The bottom of the sidebar shows a trust status badge when authenticated. These three things are related (user identity + trust status) but scattered.

## Goal

Consolidate user identity, trust status, and account actions into a single accordion at the sidebar bottom. Cleaner layout, mobile-friendly, and sets up for adding more status badges later.

## Design

### Collapsed State (default)

A single row pinned to the sidebar bottom showing:
- User icon + username
- Colored dot badge (violet/blue/green/yellow per trust tier — no text label)
- Chevron indicating it expands upward

### Expanded State (accordion opens upward)

- **Trust** → `/trust`
- **Endorse** → `/endorse`
- **Settings** → `/settings`
- *(divider)*
- **Logout** (red)

### Behavior

- Collapsed by default on page load
- Stays open across in-app navigation (not reset on route change)
- Mobile: sidebar still closes on link click as today; accordion open/closed state preserved
- Only renders when authenticated

### Layout Changes

| Location | Before | After |
|---|---|---|
| Header (top-right) | Dark/light toggle + UserMenu | Dark/light toggle only |
| Sidebar main nav | Home, Rooms, Docs, Trust, Endorse | Home, Rooms, Docs |
| Sidebar bottom (authed) | Trust tier badge (text + color) | Accordion: collapsed = icon+username+dot badge; expanded = Trust, Endorse, Settings, Logout |
| Sidebar bottom (unauthed) | Login, Sign Up | Login, Sign Up (no change) |

### Badge Dot Behavior

Reuses existing trust tier logic from `useVerificationStatus()` + `useTrustScores()`:
- `Congress` → violet dot
- `Community` → blue dot
- `Verified` → green dot
- `Unverified` (verifier URL available) → yellow dot (links to verifier URL on click? TBD in impl)
- No verifier URL → no dot

## Files to Touch

- `web/src/pages/Layout.tsx` — remove UserMenu from header
- `web/src/components/Navbar/Navbar.tsx` — remove Trust/Endorse from main nav, replace bottom section with new accordion
- `web/src/components/UserMenu/UserMenu.tsx` — can be deleted or repurposed
- Possibly a new `web/src/components/Navbar/UserAccordion.tsx` for the accordion component
