/**
 * PollDeadline — countdown with progress ring, full closing time, and UTC/local toggle
 */

import { useCallback, useEffect, useState } from 'react';
import { IconClock } from '@tabler/icons-react';
import { Group, RingProgress, Text, UnstyledButton } from '@mantine/core';
import type { Poll } from '../api';
import { formatTime } from './PollCountdown';

interface Props {
  poll: Poll;
  secondsLeft: number | null;
}

function computeProgress(poll: Poll): number | null {
  if (!poll.activated_at || !poll.closes_at) {
    return null;
  }
  const start = new Date(poll.activated_at).getTime();
  const end = new Date(poll.closes_at).getTime();
  const total = end - start;
  if (total <= 0) {
    return 100;
  }
  const elapsed = Date.now() - start;
  return Math.min(100, Math.max(0, (elapsed / total) * 100));
}

function formatFullTime(isoString: string, useUtc: boolean): string {
  const date = new Date(isoString);
  if (useUtc) {
    return date.toLocaleString('en-US', {
      timeZone: 'UTC',
      month: 'short',
      day: 'numeric',
      year: 'numeric',
      hour: 'numeric',
      minute: '2-digit',
      timeZoneName: 'short',
    });
  }
  return date.toLocaleString('en-US', {
    month: 'short',
    day: 'numeric',
    year: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
    timeZoneName: 'short',
  });
}

export function PollDeadline({ poll, secondsLeft }: Props) {
  const [useUtc, setUseUtc] = useState(false);
  const [progress, setProgress] = useState<number | null>(() => computeProgress(poll));

  useEffect(() => {
    const update = () => {
      setProgress(computeProgress(poll));
    };
    update();
    const id = setInterval(update, 1000);
    return () => {
      clearInterval(id);
    };
  }, [poll]);

  const toggleTimezone = useCallback(() => {
    setUseUtc((prev) => !prev);
  }, []);

  if (secondsLeft === null || !poll.closes_at) {
    return null;
  }

  const isUrgent = secondsLeft <= 30;
  const countdownText = secondsLeft <= 0 ? 'Closing...' : `Closes in ${formatTime(secondsLeft)}`;
  const fullTimeText = formatFullTime(poll.closes_at, useUtc);

  return (
    <Group
      gap="sm"
      bd={`1px solid ${isUrgent ? 'orange.4' : 'blue.4'}`}
      p="6px 12px"
      style={{ borderRadius: 'var(--mantine-radius-md)' }}
    >
      {progress !== null ? (
        <RingProgress
          size={28}
          thickness={3}
          roundCaps
          sections={[{ value: progress, color: isUrgent ? 'orange' : 'blue' }]}
        />
      ) : (
        <IconClock size={18} color="var(--mantine-color-blue-6)" />
      )}
      <div style={{ minWidth: 200 }}>
        <Text size="sm" fw={isUrgent ? 600 : 500} c={isUrgent ? 'orange' : undefined}>
          {countdownText}
        </Text>
        <UnstyledButton onClick={toggleTimezone}>
          <Text size="xs" c="dimmed" style={{ cursor: 'pointer' }}>
            {fullTimeText}
          </Text>
        </UnstyledButton>
      </div>
    </Group>
  );
}
