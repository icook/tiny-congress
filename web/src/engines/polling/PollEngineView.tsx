/**
 * PollEngineView — room detail page for the polling engine.
 *
 * Shows a timeline: active poll hero card, upcoming drafts, and past polls.
 */

import { Link } from '@tanstack/react-router';
import { Badge, Button, Card, Group, Loader, Stack, Text, Title } from '@mantine/core';
import type { EngineViewProps } from '../api';
import { usePolls, type Poll } from './api';
import { PollCountdown } from './components/PollCountdown';
import { SuggestionFeed } from './components/SuggestionFeed';
import { usePollCountdown } from './hooks/usePollCountdown';

function queueLabel(index: number): string {
  if (index === 0) {
    return 'Up next';
  }
  const n = index + 1;
  const suffix = n === 2 ? 'nd' : n === 3 ? 'rd' : 'th';
  return `${String(n)}${suffix} in line`;
}

export function PollEngineView({ room, roomId }: EngineViewProps) {
  const { data: polls, isLoading } = usePolls(roomId);

  const active = polls?.filter((p) => p.status === 'active') ?? [];
  const drafts = polls?.filter((p) => p.status === 'draft') ?? [];
  const closed = polls?.filter((p) => p.status === 'closed') ?? [];

  return (
    <Stack gap="lg" maw={800} mx="auto" mt="xl" px="md">
      <Group gap={6}>
        <Text component={Link} to="/rooms" size="sm" c="dimmed" td="none">
          Rooms
        </Text>
        <Text size="sm" c="dimmed">
          /
        </Text>
        <Text size="sm" c="dimmed">
          {room.name}
        </Text>
      </Group>

      <div>
        <Title order={2}>{room.name}</Title>
        {room.description ? (
          <Text c="dimmed" size="sm" mt="xs">
            {room.description}
          </Text>
        ) : null}
      </div>

      {isLoading ? <Loader size="sm" /> : null}

      {!isLoading && active.length === 0 ? (
        <Text c="dimmed" size="sm">
          No active poll — check back soon.
        </Text>
      ) : null}

      {active.map((poll) => (
        <ActivePollCard key={poll.id} roomId={roomId} poll={poll} />
      ))}

      <SuggestionFeed roomId={roomId} />

      {drafts.length > 0 ? (
        <Stack gap="xs">
          <Title order={4} c="dimmed">
            Up next ({String(drafts.length)})
          </Title>
          {drafts.map((poll, index) => (
            <Card key={poll.id} padding="sm" radius="sm" withBorder>
              <Group justify="space-between" wrap="nowrap">
                <Text size="sm">{poll.question}</Text>
                <Badge color="blue" variant="light" size="sm">
                  {queueLabel(index)}
                </Badge>
              </Group>
            </Card>
          ))}
        </Stack>
      ) : null}

      {closed.length > 0 ? (
        <Stack gap="xs">
          <Title order={4} c="dimmed">
            Past polls ({String(closed.length)})
          </Title>
          {closed.map((poll) => (
            <ClosedPollCard key={poll.id} roomId={roomId} poll={poll} />
          ))}
        </Stack>
      ) : null}
    </Stack>
  );
}

function ActivePollCard({ roomId, poll }: { roomId: string; poll: Poll }) {
  const { secondsLeft } = usePollCountdown(poll);

  return (
    <Card shadow="sm" padding="lg" radius="md" withBorder>
      <Stack gap="sm">
        <Group justify="space-between" wrap="nowrap">
          <Group gap="xs" wrap="nowrap">
            <Badge color="green" variant="filled" size="sm">
              Active
            </Badge>
            <Title order={3}>{poll.question}</Title>
          </Group>
          <PollCountdown secondsLeft={secondsLeft} />
        </Group>
        {poll.description ? (
          <Text size="sm" c="dimmed">
            {poll.description}
          </Text>
        ) : null}
        <Button
          component={Link}
          to={`/rooms/${roomId}/polls/${poll.id}`}
          variant="filled"
          size="sm"
        >
          Vote now
        </Button>
      </Stack>
    </Card>
  );
}

function ClosedPollCard({ roomId, poll }: { roomId: string; poll: Poll }) {
  return (
    <Card
      component={Link}
      to={`/rooms/${roomId}/polls/${poll.id}`}
      padding="sm"
      radius="sm"
      withBorder
      td="none"
      style={{ cursor: 'pointer', color: 'inherit' }}
    >
      <Group justify="space-between" wrap="nowrap">
        <Text size="sm">{poll.question}</Text>
        <Badge color="blue" variant="light">
          Results
        </Badge>
      </Group>
    </Card>
  );
}
