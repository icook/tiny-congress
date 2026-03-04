import { useState } from 'react';
import { IconArrowsExchange } from '@tabler/icons-react';
import { Group, Text, Tooltip, type TextProps } from '@mantine/core';

type Mode = 'local' | 'utc' | 'relative';

const NEXT_MODE: Record<Mode, Mode> = {
  local: 'utc',
  utc: 'relative',
  relative: 'local',
};

const NEXT_LABEL: Record<Mode, string> = {
  local: 'Show UTC',
  utc: 'Show relative',
  relative: 'Show local time',
};

const localFormat = new Intl.DateTimeFormat(undefined, {
  year: 'numeric',
  month: 'short',
  day: 'numeric',
  hour: '2-digit',
  minute: '2-digit',
  second: '2-digit',
  timeZoneName: 'short',
});

const utcFormat = new Intl.DateTimeFormat(undefined, {
  year: 'numeric',
  month: 'short',
  day: 'numeric',
  hour: '2-digit',
  minute: '2-digit',
  second: '2-digit',
  timeZoneName: 'short',
  timeZone: 'UTC',
});

const UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
  ['year', 365 * 24 * 60 * 60 * 1000],
  ['month', 30 * 24 * 60 * 60 * 1000],
  ['week', 7 * 24 * 60 * 60 * 1000],
  ['day', 24 * 60 * 60 * 1000],
  ['hour', 60 * 60 * 1000],
  ['minute', 60 * 1000],
  ['second', 1000],
];

const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' });

function timeAgo(date: Date): string {
  const diff = date.getTime() - Date.now();
  for (const [unit, ms] of UNITS) {
    if (Math.abs(diff) >= ms) {
      return rtf.format(Math.round(diff / ms), unit);
    }
  }
  return rtf.format(0, 'second');
}

function formatTimestamp(date: Date, mode: Mode): string {
  switch (mode) {
    case 'local':
      return localFormat.format(date);
    case 'utc':
      return utcFormat.format(date);
    case 'relative':
      return timeAgo(date);
  }
}

export interface TimestampTextProps extends Omit<TextProps, 'children'> {
  value: string;
  defaultMode?: Mode;
  'data-testid'?: string;
}

export function TimestampText({
  value,
  defaultMode = 'local',
  'data-testid': testId,
  ...textProps
}: TimestampTextProps) {
  const [mode, setMode] = useState<Mode>(defaultMode);
  const [hovered, setHovered] = useState(false);

  const date = new Date(value);
  const valid = !isNaN(date.getTime());

  const cycle = () => {
    if (!valid) {
      return;
    }
    setMode((m) => NEXT_MODE[m]);
  };

  const display = valid ? formatTimestamp(date, mode) : value;

  return (
    <Tooltip label={valid ? NEXT_LABEL[mode] : 'Invalid date'} openDelay={300}>
      <Group
        gap={4}
        wrap="nowrap"
        data-testid={testId}
        role={valid ? 'button' : undefined}
        tabIndex={valid ? 0 : undefined}
        style={{ cursor: valid ? 'pointer' : undefined, display: 'inline-flex' }}
        onClick={cycle}
        onKeyDown={
          valid
            ? (e: React.KeyboardEvent) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault();
                  cycle();
                }
              }
            : undefined
        }
        onMouseEnter={() => {
          setHovered(true);
        }}
        onMouseLeave={() => {
          setHovered(false);
        }}
      >
        <Text {...textProps}>{display}</Text>
        {valid && hovered ? (
          <IconArrowsExchange size={14} style={{ opacity: 0.5, flexShrink: 0 }} />
        ) : null}
      </Group>
    </Tooltip>
  );
}
