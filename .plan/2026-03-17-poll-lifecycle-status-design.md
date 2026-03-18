# Poll Lifecycle Status — Design

**Issue:** #660
**Date:** 2026-03-17
**Branch:** feature/660-poll-lifecycle-status

## Problem

Backend exposes `closes_at` on polls and `GET /rooms/{room_id}/agenda`, but nothing is surfaced in the UI. Users can't tell how much time is left, what question is coming, or that the room is progressing through an agenda.

## Scope

- Countdown / time remaining on active poll (driven by `closes_at`)
- Agenda progress indicator ("Question N of M")
- Transition UX when poll closes (auto-refresh, no toast/overlay)
- Upcoming poll preview (next question in agenda)

## Approach: React Query + shared hooks

Extend existing query infrastructure. React Query deduplicates fetches across components sharing the same query key. No new state management patterns.

---

## Data Layer

### Type change
Add `closes_at: string | null` to `Poll` interface in `web/src/features/rooms/api/client.ts`.

### New API function
`getAgenda(roomId: string): Promise<Poll[]>` — calls `GET /rooms/{roomId}/agenda`.

### New query hook
`useAgenda(roomId)` in `queries.ts`, following `usePolls` pattern, `refetchInterval: 20_000`.

### Existing hook change
`usePollDetail` — add `refetchInterval: 20_000` (currently missing; drives auto-transition when status flips).

### New utility hook
`usePollCountdown(poll: Poll | undefined)` — returns `{ secondsLeft: number | null, isExpired: boolean }`. Uses `setInterval(1000)` ticking from `closes_at` to now. Returns `null` if `closes_at` is null. Clears interval on unmount.

---

## Components

All presentational (no data fetching), pure props-in / render-out.

### `PollCountdown`
Props: `secondsLeft: number | null`
Renders nothing if null. Displays `MM:SS`. Red text + pulse animation when < 30s remaining.

### `AgendaProgress`
Props: `polls: Poll[], activePollId: string`
Renders "Question N of M". Renders nothing if agenda has ≤ 1 poll.

### `UpcomingPollPreview`
Props: `poll: Poll | undefined`
Shows "Up next: [question]" with muted styling. Renders nothing if no next poll.

---

## Placement

| Surface | Components |
|---------|------------|
| `Poll.page.tsx` | `PollCountdown` + `AgendaProgress` + `UpcomingPollPreview` below poll question header |
| `Rooms.page.tsx` `RoomCard` | `PollCountdown` inline with active poll question |
| `Navbar.tsx` | `PollCountdown` badge when on `/rooms/:roomId/polls/:pollId` route |

---

## Data Flow

**Poll.page.tsx:**
- `usePollDetail` (add `refetchInterval: 20_000`) → poll + status
- `useAgenda(roomId)` → derive active index + next poll
- `usePollCountdown(poll)` → `secondsLeft` passed to `PollCountdown`
- When `poll.status` flips `active → closed`, next refetch returns updated state; page re-renders naturally (no navigation)

**Rooms.page.tsx:**
- `listPolls` already fetches active polls — add `closes_at` to type, pass to `PollCountdown` in `RoomCard`
- `usePollCountdown` runs per card

**Navbar:**
- Only renders countdown when on a poll route
- Calls `usePollDetail(roomId, pollId)` — already cached by React Query, no extra request
- Passes poll to `usePollCountdown` → renders `PollCountdown` badge

**Transition:**
- No toast, no navigation
- `usePollDetail` refetches every 20s; when backend marks poll `closed`, next refetch returns updated status
- Poll results section already renders conditionally on status — UI shifts naturally

---

## Non-goals

- Facilitator UI for configuring cadence / reordering agenda (separate ticket)
- Manual vs auto-rotation toggle (all rooms are auto for now)
