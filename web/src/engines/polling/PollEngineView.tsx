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
import { usePollCountdown } from './hooks/usePollCountdown';

export function PollEngineView({ room, roomId }: EngineViewProps) {
  const { data: polls, isLoading } = usePolls(roomId);

  const active = polls?.filter((p) => p.status === 'active') ?? [];
  const drafts = polls?.filter((p) => p.status === 'draft') ?? [];
  const closed = polls?.filter((p) => p.status === 'closed') ?? [];

  return (
    <Stack gap="lg" maw={800} mx="auto" mt="xl">
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

      {drafts.length > 0 ? (
        <Stack gap="xs">
          <Title order={4} c="dimmed">
            Up next ({String(drafts.length)})
          </Title>
          {drafts.map((poll) => (
            <Card key={poll.id} padding="sm" radius="sm" withBorder>
              <Text size="sm">{poll.question}</Text>
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
          <Title order={3}>{poll.question}</Title>
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
      style={{ cursor: 'pointer', textDecoration: 'none', color: 'inherit' }}
    >
      <Group justify="space-between" wrap="nowrap">
        <Text size="sm">{poll.question}</Text>
        <Badge color="gray" variant="light">
          Results
        </Badge>
      </Group>
    </Card>
  );
}
