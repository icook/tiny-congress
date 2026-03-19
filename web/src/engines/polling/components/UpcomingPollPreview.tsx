import { IconFlask } from '@tabler/icons-react';
import { Link } from '@tanstack/react-router';
import { Card, Group, Loader, Stack, Text } from '@mantine/core';
import type { Poll } from '../api';
import { usePollDetail } from '../api/queries';

interface Props {
  poll: Poll | undefined;
  roomId: string;
}

export function UpcomingPollPreview({ poll, roomId }: Props) {
  const detailQuery = usePollDetail(roomId, poll?.id ?? '');

  if (!poll) {
    return null;
  }

  const evidence = detailQuery.data?.dimensions.flatMap((d) => d.evidence) ?? [];
  const proCount = evidence.filter((e) => e.stance === 'pro').length;
  const conCount = evidence.filter((e) => e.stance === 'con').length;

  return (
    <Card
      component={Link}
      to={`/rooms/${roomId}/polls/${poll.id}`}
      withBorder
      padding="sm"
      radius="md"
      style={{
        borderStyle: 'dashed',
        textDecoration: 'none',
        cursor: 'pointer',
      }}
    >
      <Stack gap={4}>
        <Group justify="space-between">
          <Text size="xs" tt="uppercase" fw={600} c="dimmed">
            Up next
          </Text>
          {detailQuery.isLoading ? <Loader size={12} /> : null}
          {evidence.length > 0 ? (
            <Group gap={4}>
              <IconFlask size={12} color="var(--mantine-color-blue-6)" />
              <Text size="xs" c="blue" fw={500}>
                {String(proCount)} pro / {String(conCount)} con
              </Text>
            </Group>
          ) : null}
        </Group>
        <Text size="sm" fw={500}>
          {poll.question}
        </Text>
        {poll.description ? (
          <Text size="xs" c="dimmed" lineClamp={2}>
            {poll.description}
          </Text>
        ) : null}
      </Stack>
    </Card>
  );
}
