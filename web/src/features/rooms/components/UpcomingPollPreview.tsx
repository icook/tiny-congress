import { Stack, Text } from '@mantine/core';
import type { Poll } from '../api';

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
