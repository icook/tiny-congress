import { useEffect, useState } from 'react';
import { Text } from '@mantine/core';

interface Props {
  deadline: string;
  label: string;
}

function formatTimeRemaining(seconds: number): string {
  if (seconds <= 0) {
    return 'Closed';
  }
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = seconds % 60;
  if (h > 0) {
    return `${String(h)}h ${String(m)}m ${String(s)}s`;
  }
  if (m > 0) {
    return `${String(m)}m ${String(s)}s`;
  }
  return `${String(s)}s`;
}

export function RoundCountdown({ deadline, label }: Props) {
  const [secondsLeft, setSecondsLeft] = useState<number | null>(null);

  useEffect(() => {
    const update = () => {
      const ms = new Date(deadline).getTime() - Date.now();
      setSecondsLeft(Math.max(0, Math.floor(ms / 1000)));
    };

    update();
    const id = setInterval(update, 1000);
    return () => {
      clearInterval(id);
    };
  }, [deadline]);

  if (secondsLeft === null) {
    return null;
  }

  const isClosed = secondsLeft <= 0;
  const isUrgent = secondsLeft > 0 && secondsLeft <= 60;

  return (
    <Text
      size="sm"
      c={isClosed ? 'dimmed' : isUrgent ? 'red' : 'dimmed'}
      fw={isUrgent ? 600 : undefined}
    >
      {label}: {isClosed ? 'Closed' : formatTimeRemaining(secondsLeft)}
    </Text>
  );
}
