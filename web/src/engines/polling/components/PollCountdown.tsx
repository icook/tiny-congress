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
