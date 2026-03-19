import { Text } from '@mantine/core';

interface Props {
  secondsLeft: number | null;
}

export function formatTime(seconds: number): string {
  if (seconds >= 86400) {
    const d = Math.floor(seconds / 86400);
    const h = Math.floor((seconds % 86400) / 3600);
    return h > 0 ? `${String(d)}d ${String(h)}h` : `${String(d)}d`;
  }
  if (seconds >= 3600) {
    const h = Math.floor(seconds / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    return m > 0 ? `${String(h)}h ${String(m)}m` : `${String(h)}h`;
  }
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
