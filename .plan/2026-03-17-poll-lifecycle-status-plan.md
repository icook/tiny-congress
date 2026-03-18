# Poll Lifecycle Status Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Surface poll countdown, agenda progress, and upcoming poll preview across the poll page, room cards, and navbar.

**Architecture:** Extend existing React Query infrastructure — add `closes_at` to `Poll` type, add `getAgenda`/`useAgenda` following existing patterns, and add a `usePollCountdown` hook. Three pure presentational components render the data. No new state management.

**Tech Stack:** React, TanStack Query, Mantine UI, TypeScript, Vitest + Testing Library

---

### Task 1: Add `closes_at` to `Poll` type and `getAgenda` client function

**Files:**
- Modify: `web/src/features/rooms/api/client.ts`

**Step 1: Add `closes_at` to the `Poll` interface**

Find the `Poll` interface (lines 21-28) and add the field:

```typescript
export interface Poll {
  id: string;
  room_id: string;
  question: string;
  description: string | null;
  status: string;
  created_at: string;
  closes_at: string | null;
}
```

**Step 2: Add `getAgenda` function**

After the `listPolls` function (after line 101), add:

```typescript
export async function getAgenda(roomId: string): Promise<Poll[]> {
  return fetchJson(`/rooms/${roomId}/agenda`);
}
```

**Step 3: Verify TypeScript compiles**

```bash
cd web && yarn tsc --noEmit
```

Expected: no errors.

**Step 4: Commit**

```bash
git add web/src/features/rooms/api/client.ts
git commit -m "feat(rooms): add closes_at to Poll type and getAgenda client"
```

---

### Task 2: Add `useAgenda` hook and `refetchInterval` to `usePollDetail`

**Files:**
- Modify: `web/src/features/rooms/api/queries.ts`
- Modify: `web/src/features/rooms/index.ts`

**Step 1: Add `getAgenda` import and `useAgenda` hook**

In `queries.ts`, add `getAgenda` to the import from `./client`:

```typescript
import {
  castVote,
  getAgenda,          // add this
  getMyVotes,
  getPollDetail,
  getPollDistribution,
  getPollResults,
  getRoom,
  listPolls,
  listRooms,
  // ... rest of imports
} from './client';
```

Also add `Poll` to type imports if not already present (it is already there via `type Poll`).

**Step 2: Add `refetchInterval` to `usePollDetail`**

Find `usePollDetail` (lines 48-54) and add the option:

```typescript
export function usePollDetail(roomId: string, pollId: string) {
  return useQuery<PollDetail>({
    queryKey: ['poll-detail', pollId],
    queryFn: () => getPollDetail(roomId, pollId),
    enabled: Boolean(roomId && pollId),
    refetchInterval: 20_000,
  });
}
```

**Step 3: Add `useAgenda` hook**

After `usePollDetail`, add:

```typescript
export function useAgenda(roomId: string) {
  return useQuery<Poll[]>({
    queryKey: ['agenda', roomId],
    queryFn: () => getAgenda(roomId),
    enabled: Boolean(roomId),
    refetchInterval: 20_000,
  });
}
```

**Step 4: Export from `index.ts`**

In `web/src/features/rooms/index.ts`, add `useAgenda` to the exports:

```typescript
export {
  useRooms,
  useRoom,
  usePolls,
  usePollDetail,
  useAgenda,          // add this
  usePollResults,
  usePollDistribution,
  useMyVotes,
  useCastVote,
} from './api/queries';
```

**Step 5: Verify TypeScript compiles**

```bash
cd web && yarn tsc --noEmit
```

**Step 6: Commit**

```bash
git add web/src/features/rooms/api/queries.ts web/src/features/rooms/index.ts
git commit -m "feat(rooms): add useAgenda hook and auto-refetch to usePollDetail"
```

---

### Task 3: `usePollCountdown` hook

**Files:**
- Create: `web/src/features/rooms/hooks/usePollCountdown.ts`
- Create: `web/src/features/rooms/hooks/usePollCountdown.test.ts`
- Modify: `web/src/features/rooms/index.ts`

**Step 1: Write the failing test**

Create `web/src/features/rooms/hooks/usePollCountdown.test.ts`:

```typescript
import { renderHook, act } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { vi } from 'vitest';
import { usePollCountdown } from './usePollCountdown';
import type { Poll } from '../api/client';

function makePoll(closesAt: string | null): Poll {
  return {
    id: '1',
    room_id: 'r1',
    question: 'Q?',
    description: null,
    status: 'active',
    created_at: new Date().toISOString(),
    closes_at: closesAt,
  };
}

describe('usePollCountdown', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('returns null when poll is undefined', () => {
    const { result } = renderHook(() => usePollCountdown(undefined));
    expect(result.current.secondsLeft).toBeNull();
    expect(result.current.isExpired).toBe(false);
  });

  it('returns null when poll has no closes_at', () => {
    const { result } = renderHook(() => usePollCountdown(makePoll(null)));
    expect(result.current.secondsLeft).toBeNull();
    expect(result.current.isExpired).toBe(false);
  });

  it('returns seconds remaining when closes_at is in the future', () => {
    const future = new Date(Date.now() + 60_000).toISOString();
    const { result } = renderHook(() => usePollCountdown(makePoll(future)));
    expect(result.current.secondsLeft).toBe(60);
    expect(result.current.isExpired).toBe(false);
  });

  it('returns isExpired true when closes_at is in the past', () => {
    const past = new Date(Date.now() - 1000).toISOString();
    const { result } = renderHook(() => usePollCountdown(makePoll(past)));
    expect(result.current.secondsLeft).toBe(0);
    expect(result.current.isExpired).toBe(true);
  });

  it('ticks down over time', () => {
    const future = new Date(Date.now() + 10_000).toISOString();
    const { result } = renderHook(() => usePollCountdown(makePoll(future)));

    act(() => {
      vi.advanceTimersByTime(3_000);
    });

    expect(result.current.secondsLeft).toBe(7);
  });
});
```

**Step 2: Run test to verify it fails**

```bash
cd web && yarn vitest src/features/rooms/hooks/usePollCountdown.test.ts
```

Expected: FAIL — `usePollCountdown` not found.

**Step 3: Implement `usePollCountdown`**

Create `web/src/features/rooms/hooks/usePollCountdown.ts`:

```typescript
import { useEffect, useState } from 'react';
import type { Poll } from '../api/client';

export interface CountdownState {
  secondsLeft: number | null;
  isExpired: boolean;
}

export function usePollCountdown(poll: Poll | undefined): CountdownState {
  const [secondsLeft, setSecondsLeft] = useState<number | null>(null);

  const closesAt = poll?.closes_at ?? null;

  useEffect(() => {
    if (!closesAt) {
      setSecondsLeft(null);
      return;
    }

    const update = () => {
      const ms = new Date(closesAt).getTime() - Date.now();
      setSecondsLeft(Math.max(0, Math.floor(ms / 1000)));
    };

    update();
    const id = setInterval(update, 1000);
    return () => clearInterval(id);
  }, [closesAt]);

  return {
    secondsLeft,
    isExpired: secondsLeft !== null && secondsLeft <= 0,
  };
}
```

**Step 4: Run test to verify it passes**

```bash
cd web && yarn vitest src/features/rooms/hooks/usePollCountdown.test.ts
```

Expected: all 5 tests PASS.

**Step 5: Export from `index.ts`**

Add to `web/src/features/rooms/index.ts`:

```typescript
export { usePollCountdown } from './hooks/usePollCountdown';
export type { CountdownState } from './hooks/usePollCountdown';
```

**Step 6: Commit**

```bash
git add web/src/features/rooms/hooks/ web/src/features/rooms/index.ts
git commit -m "feat(rooms): add usePollCountdown hook"
```

---

### Task 4: `PollCountdown` component

**Files:**
- Create: `web/src/features/rooms/components/PollCountdown.tsx`
- Create: `web/src/features/rooms/components/PollCountdown.test.tsx`
- Modify: `web/src/features/rooms/index.ts`

**Step 1: Write the failing test**

Create `web/src/features/rooms/components/PollCountdown.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import { MantineProvider } from '@mantine/core';
import { describe, expect, it } from 'vitest';
import { PollCountdown } from './PollCountdown';

function wrap(ui: React.ReactElement) {
  return render(<MantineProvider>{ui}</MantineProvider>);
}

describe('PollCountdown', () => {
  it('renders nothing when secondsLeft is null', () => {
    const { container } = wrap(<PollCountdown secondsLeft={null} />);
    expect(container).toBeEmptyDOMElement();
  });

  it('displays formatted time for > 60 seconds', () => {
    wrap(<PollCountdown secondsLeft={90} />);
    expect(screen.getByText('Closes in 01:30')).toBeInTheDocument();
  });

  it('displays formatted time for < 60 seconds', () => {
    wrap(<PollCountdown secondsLeft={45} />);
    expect(screen.getByText('Closes in 00:45')).toBeInTheDocument();
  });

  it('displays closing message when secondsLeft is 0', () => {
    wrap(<PollCountdown secondsLeft={0} />);
    expect(screen.getByText('Closing...')).toBeInTheDocument();
  });
});
```

**Step 2: Run test to verify it fails**

```bash
cd web && yarn vitest src/features/rooms/components/PollCountdown.test.tsx
```

Expected: FAIL — `PollCountdown` not found.

**Step 3: Implement `PollCountdown`**

Create `web/src/features/rooms/components/PollCountdown.tsx`:

```tsx
import { Text } from '@mantine/core';

interface Props {
  secondsLeft: number | null;
}

function formatTime(seconds: number): string {
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return `${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
}

export function PollCountdown({ secondsLeft }: Props) {
  if (secondsLeft === null) {
    return null;
  }

  if (secondsLeft <= 0) {
    return (
      <Text size="sm" c="red" fw={600}>
        Closing...
      </Text>
    );
  }

  const isUrgent = secondsLeft <= 30;

  return (
    <Text size="sm" c={isUrgent ? 'red' : 'dimmed'} fw={isUrgent ? 600 : undefined}>
      Closes in {formatTime(secondsLeft)}
    </Text>
  );
}
```

**Step 4: Run test to verify it passes**

```bash
cd web && yarn vitest src/features/rooms/components/PollCountdown.test.tsx
```

Expected: all 4 tests PASS.

**Step 5: Export from `index.ts`**

Add to `web/src/features/rooms/index.ts`:

```typescript
export { PollCountdown } from './components/PollCountdown';
```

**Step 6: Commit**

```bash
git add web/src/features/rooms/components/PollCountdown.tsx web/src/features/rooms/components/PollCountdown.test.tsx web/src/features/rooms/index.ts
git commit -m "feat(rooms): add PollCountdown component"
```

---

### Task 5: `AgendaProgress` component

**Files:**
- Create: `web/src/features/rooms/components/AgendaProgress.tsx`
- Create: `web/src/features/rooms/components/AgendaProgress.test.tsx`
- Modify: `web/src/features/rooms/index.ts`

**Step 1: Write the failing test**

Create `web/src/features/rooms/components/AgendaProgress.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import { MantineProvider } from '@mantine/core';
import { describe, expect, it } from 'vitest';
import { AgendaProgress } from './AgendaProgress';
import type { Poll } from '../api/client';

function wrap(ui: React.ReactElement) {
  return render(<MantineProvider>{ui}</MantineProvider>);
}

function makePoll(id: string): Poll {
  return { id, room_id: 'r1', question: `Q${id}`, description: null, status: 'active', created_at: '', closes_at: null };
}

describe('AgendaProgress', () => {
  it('renders nothing for a single poll', () => {
    const { container } = wrap(
      <AgendaProgress polls={[makePoll('a')]} activePollId="a" />
    );
    expect(container).toBeEmptyDOMElement();
  });

  it('shows first question of N', () => {
    const polls = ['a', 'b', 'c'].map(makePoll);
    wrap(<AgendaProgress polls={polls} activePollId="a" />);
    expect(screen.getByText('Question 1 of 3')).toBeInTheDocument();
  });

  it('shows correct position when active is not first', () => {
    const polls = ['a', 'b', 'c'].map(makePoll);
    wrap(<AgendaProgress polls={polls} activePollId="c" />);
    expect(screen.getByText('Question 3 of 3')).toBeInTheDocument();
  });

  it('renders nothing when activePollId is not in the list', () => {
    const polls = ['a', 'b'].map(makePoll);
    const { container } = wrap(<AgendaProgress polls={polls} activePollId="z" />);
    expect(container).toBeEmptyDOMElement();
  });
});
```

**Step 2: Run test to verify it fails**

```bash
cd web && yarn vitest src/features/rooms/components/AgendaProgress.test.tsx
```

Expected: FAIL.

**Step 3: Implement `AgendaProgress`**

Create `web/src/features/rooms/components/AgendaProgress.tsx`:

```tsx
import { Text } from '@mantine/core';
import type { Poll } from '../api/client';

interface Props {
  polls: Poll[];
  activePollId: string;
}

export function AgendaProgress({ polls, activePollId }: Props) {
  if (polls.length <= 1) {
    return null;
  }

  const index = polls.findIndex((p) => p.id === activePollId);
  if (index === -1) {
    return null;
  }

  return (
    <Text size="sm" c="dimmed">
      Question {index + 1} of {polls.length}
    </Text>
  );
}
```

**Step 4: Run test to verify it passes**

```bash
cd web && yarn vitest src/features/rooms/components/AgendaProgress.test.tsx
```

Expected: all 4 tests PASS.

**Step 5: Export from `index.ts`**

```typescript
export { AgendaProgress } from './components/AgendaProgress';
```

**Step 6: Commit**

```bash
git add web/src/features/rooms/components/AgendaProgress.tsx web/src/features/rooms/components/AgendaProgress.test.tsx web/src/features/rooms/index.ts
git commit -m "feat(rooms): add AgendaProgress component"
```

---

### Task 6: `UpcomingPollPreview` component

**Files:**
- Create: `web/src/features/rooms/components/UpcomingPollPreview.tsx`
- Create: `web/src/features/rooms/components/UpcomingPollPreview.test.tsx`
- Modify: `web/src/features/rooms/index.ts`

**Step 1: Write the failing test**

Create `web/src/features/rooms/components/UpcomingPollPreview.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import { MantineProvider } from '@mantine/core';
import { describe, expect, it } from 'vitest';
import { UpcomingPollPreview } from './UpcomingPollPreview';
import type { Poll } from '../api/client';

function wrap(ui: React.ReactElement) {
  return render(<MantineProvider>{ui}</MantineProvider>);
}

function makePoll(question: string): Poll {
  return { id: '1', room_id: 'r1', question, description: null, status: 'pending', created_at: '', closes_at: null };
}

describe('UpcomingPollPreview', () => {
  it('renders nothing when poll is undefined', () => {
    const { container } = wrap(<UpcomingPollPreview poll={undefined} />);
    expect(container).toBeEmptyDOMElement();
  });

  it('renders the up next label and question', () => {
    wrap(<UpcomingPollPreview poll={makePoll('Should we build a park?')} />);
    expect(screen.getByText('Up next')).toBeInTheDocument();
    expect(screen.getByText('Should we build a park?')).toBeInTheDocument();
  });
});
```

**Step 2: Run test to verify it fails**

```bash
cd web && yarn vitest src/features/rooms/components/UpcomingPollPreview.test.tsx
```

Expected: FAIL.

**Step 3: Implement `UpcomingPollPreview`**

Create `web/src/features/rooms/components/UpcomingPollPreview.tsx`:

```tsx
import { Stack, Text } from '@mantine/core';
import type { Poll } from '../api/client';

interface Props {
  poll: Poll | undefined;
}

export function UpcomingPollPreview({ poll }: Props) {
  if (!poll) {
    return null;
  }

  return (
    <Stack gap={2}>
      <Text size="xs" c="dimmed" fw={500}>
        Up next
      </Text>
      <Text size="sm" c="dimmed">
        {poll.question}
      </Text>
    </Stack>
  );
}
```

**Step 4: Run test to verify it passes**

```bash
cd web && yarn vitest src/features/rooms/components/UpcomingPollPreview.test.tsx
```

Expected: all 2 tests PASS.

**Step 5: Export from `index.ts`**

```typescript
export { UpcomingPollPreview } from './components/UpcomingPollPreview';
```

**Step 6: Commit**

```bash
git add web/src/features/rooms/components/UpcomingPollPreview.tsx web/src/features/rooms/components/UpcomingPollPreview.test.tsx web/src/features/rooms/index.ts
git commit -m "feat(rooms): add UpcomingPollPreview component"
```

---

### Task 7: Wire into `Poll.page.tsx`

**Files:**
- Modify: `web/src/pages/Poll.page.tsx`

**Step 1: Add imports**

At the top of `Poll.page.tsx`, add to the `@/features/rooms` import:

```typescript
import {
  useAgenda,
  usePollCountdown,
  AgendaProgress,
  PollCountdown,
  UpcomingPollPreview,
  useCastVote,
  useMyVotes,
  usePollDetail,
  usePollDistribution,
  usePollResults,
  useRoom,
  type Dimension,
  type DimensionVote,
} from '@/features/rooms';
```

**Step 2: Add hooks in `PollPage`**

After the existing hook calls (after line ~53), add:

```typescript
const agendaQuery = useAgenda(roomId);
const { secondsLeft } = usePollCountdown(detailQuery.data?.poll);
```

**Step 3: Derive agenda helpers**

After the `const { poll, dimensions } = detailQuery.data;` line (around line 116), add:

```typescript
const agenda = agendaQuery.data ?? [];
const activePollIndex = agenda.findIndex((p) => p.id === poll.id);
const nextPoll = activePollIndex >= 0 ? agenda[activePollIndex + 1] : undefined;
```

**Step 4: Render lifecycle status below the poll title**

Find the `<div>` containing `<Title order={2}>{poll.question}</Title>` (around line 155). After the closing `</div>` of that block (after the description text), add:

```tsx
{isActive ? (
  <Group gap="md" mt="xs">
    <PollCountdown secondsLeft={secondsLeft} />
    <AgendaProgress polls={agenda} activePollId={poll.id} />
  </Group>
) : null}
{isActive ? <UpcomingPollPreview poll={nextPoll} /> : null}
```

`Group` is already imported from `@mantine/core`. Verify it's in the destructured imports at the top of the file.

**Step 5: Verify TypeScript compiles**

```bash
cd web && yarn tsc --noEmit
```

**Step 6: Run frontend tests**

```bash
cd web && yarn vitest
```

Expected: all tests pass (Poll.page.tsx has no unit tests; just verify existing tests don't break).

**Step 7: Commit**

```bash
git add web/src/pages/Poll.page.tsx
git commit -m "feat(poll-page): show countdown, agenda progress, and upcoming preview"
```

---

### Task 8: Wire countdown into `Rooms.page.tsx`

**Files:**
- Modify: `web/src/pages/Rooms.page.tsx`

**Step 1: Add imports**

Update the `@/features/rooms` import:

```typescript
import { PollCountdown, usePollCountdown, usePolls, useRooms, type Poll, type Room } from '@/features/rooms';
```

**Step 2: Update `RoomCard` to pass `Poll` to a new `PollListItem`**

Replace the inner `activePolls.map` block. Currently (around line 82-93):

```tsx
{activePolls.map((poll) => (
  <Card
    key={poll.id}
    component={Link}
    to={`/rooms/${room.id}/polls/${poll.id}`}
    padding="sm"
    radius="sm"
    withBorder
    style={{ cursor: 'pointer', textDecoration: 'none' }}
  >
    <Text size="sm">{poll.question}</Text>
  </Card>
))}
```

Replace with:

```tsx
{activePolls.map((poll) => (
  <PollListItem key={poll.id} roomId={room.id} poll={poll} />
))}
```

**Step 3: Add `PollListItem` component at the bottom of the file**

After the closing brace of `RoomCard`, add:

```tsx
function PollListItem({ roomId, poll }: { roomId: string; poll: Poll }) {
  const { secondsLeft } = usePollCountdown(poll);

  return (
    <Card
      component={Link}
      to={`/rooms/${roomId}/polls/${poll.id}`}
      padding="sm"
      radius="sm"
      withBorder
      style={{ cursor: 'pointer', textDecoration: 'none' }}
    >
      <Group justify="space-between" wrap="nowrap">
        <Text size="sm">{poll.question}</Text>
        <PollCountdown secondsLeft={secondsLeft} />
      </Group>
    </Card>
  );
}
```

**Step 4: Add `Group` to Mantine imports if not already there**

Check the Mantine import line at the top — add `Group` if missing:

```typescript
import { Alert, Badge, Card, Group, Loader, Stack, Text, Title } from '@mantine/core';
```

**Step 5: Verify TypeScript compiles**

```bash
cd web && yarn tsc --noEmit
```

**Step 6: Commit**

```bash
git add web/src/pages/Rooms.page.tsx
git commit -m "feat(rooms-page): show countdown on active poll cards"
```

---

### Task 9: Wire countdown into `Navbar.tsx`

**Files:**
- Modify: `web/src/components/Navbar/Navbar.tsx`

**Step 1: Add imports**

At the top of `Navbar.tsx`:

```typescript
import { PollCountdown, usePollCountdown, usePollDetail } from '@/features/rooms';
```

**Step 2: Extract poll route params from current path**

Inside the `Navbar` function body, after the `currentPath` declaration, add:

```typescript
const pollRouteMatch = /^\/rooms\/([^/]+)\/polls\/([^/]+)/.exec(currentPath);
const pollRouteRoomId = pollRouteMatch?.[1];
const pollRoutePollId = pollRouteMatch?.[2];
```

**Step 3: Add `NavbarPollCountdown` below the nav links stack**

In the JSX, after the closing `</Stack>` of `gap={4}` nav links (around line 53-119), and before the auth section at the bottom, add:

```tsx
{pollRouteRoomId && pollRoutePollId ? (
  <NavbarPollCountdown roomId={pollRouteRoomId} pollId={pollRoutePollId} />
) : null}
```

**Step 4: Add `NavbarPollCountdown` component at the bottom of the file**

After the closing brace of `Navbar`:

```tsx
function NavbarPollCountdown({ roomId, pollId }: { roomId: string; pollId: string }) {
  const detailQuery = usePollDetail(roomId, pollId);
  const { secondsLeft } = usePollCountdown(detailQuery.data?.poll);

  if (secondsLeft === null) {
    return null;
  }

  return (
    <Box px="sm" py="xs">
      <Text size="xs" c="dimmed" mb={2}>
        Active poll
      </Text>
      <PollCountdown secondsLeft={secondsLeft} />
    </Box>
  );
}
```

`Box` and `Text` are already imported from `@mantine/core` — verify they're in the import list.

**Step 5: Verify TypeScript compiles**

```bash
cd web && yarn tsc --noEmit
```

**Step 6: Commit**

```bash
git add web/src/components/Navbar/Navbar.tsx
git commit -m "feat(navbar): show active poll countdown when on poll route"
```

---

### Task 10: Full lint and test pass

**Step 1: Run all frontend tests**

```bash
cd web && yarn vitest --run
```

Expected: all tests pass.

**Step 2: Run linting**

```bash
just lint-frontend
```

Expected: no errors, no warnings.

**Step 3: Run type check**

```bash
cd web && yarn tsc --noEmit
```

Expected: no errors.

**Step 4: Fix any lint issues**

If `prefer-nullish-coalescing` warnings appear, replace `||` with `??` in your new code.

If deep import lint errors appear, ensure all imports use `@/features/rooms` (the barrel) not `@/features/rooms/api/client`.

**Step 5: Commit any lint fixes**

```bash
git add -p
git commit -m "fix(rooms): address lint issues in poll lifecycle components"
```
