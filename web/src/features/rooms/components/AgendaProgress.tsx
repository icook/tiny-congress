import { Text } from '@mantine/core';
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

  return (
    <Text size="sm" c="dimmed">
      Question {index + 1} of {polls.length}
    </Text>
  );
}
