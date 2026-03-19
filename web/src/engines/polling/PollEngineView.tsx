/**
 * PollEngineView — engine-compatible wrapper for the polling UI.
 *
 * This is a thin placeholder. The full extraction of Poll.page.tsx content
 * into this component is a future task. For now, it renders a minimal stub
 * so the engine registry has a valid EngineView export.
 */

import { Center, Text } from '@mantine/core';
import type { EngineViewProps } from '../api';

export function PollEngineView({ room }: EngineViewProps) {
  return (
    <Center>
      <Text c="dimmed">Poll engine view for room: {room.name}</Text>
    </Center>
  );
}
