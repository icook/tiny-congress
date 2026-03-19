import { Group, Progress, Text } from '@mantine/core';

interface SlotCounterProps {
  used: number;
  total: number;
  outOfSlot?: number;
}

export function SlotCounter({ used, total, outOfSlot }: SlotCounterProps) {
  const pct = total > 0 ? (used / total) * 100 : 0;
  const color = used >= total ? 'red' : used >= total - 1 ? 'yellow' : 'green';
  const hasOutOfSlot = outOfSlot != null && outOfSlot > 0;

  return (
    <div>
      <Group justify="space-between" mb={4}>
        <Text size="sm" fw={500}>
          Endorsement slots
        </Text>
        <Text size="sm" c="dimmed">
          {used} of {total} in-slot{hasOutOfSlot ? ` + ${String(outOfSlot)} additional` : ''}
        </Text>
      </Group>
      <Progress value={pct} color={color} size="sm" radius="xl" />
    </div>
  );
}
