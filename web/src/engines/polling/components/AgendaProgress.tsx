import { Group, Progress, Text } from '@mantine/core';
import type { Poll } from '../api';

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

  const pct = ((index + 1) / polls.length) * 100;

  return (
    <Group gap="xs" style={{ flex: 1 }}>
      <Text size="xs" c="dimmed" fw={500} style={{ whiteSpace: 'nowrap' }}>
        {index + 1} / {polls.length}
      </Text>
      <Progress value={pct} size="sm" style={{ flex: 1 }} color="blue" />
    </Group>
  );
}
