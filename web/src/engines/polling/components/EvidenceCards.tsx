/**
 * EvidenceCards — pro/con evidence items for a dimension, shown by default
 */

import { IconFlask } from '@tabler/icons-react';
import { Collapse, Group, Stack, Text, UnstyledButton } from '@mantine/core';
import { useDisclosure } from '@mantine/hooks';
import type { Evidence } from '../api';

interface EvidenceCardsProps {
  evidence: Evidence[];
}

export function EvidenceCards({ evidence }: EvidenceCardsProps) {
  const [opened, { toggle }] = useDisclosure(true);

  if (evidence.length === 0) {
    return null;
  }

  return (
    <div>
      <UnstyledButton onClick={toggle}>
        <Group gap={4}>
          <IconFlask size={14} color="var(--mantine-color-blue-6)" />
          <Text size="xs" c="blue" fw={500} style={{ cursor: 'pointer' }}>
            Research ({String(evidence.length)}) {opened ? '▲' : '▼'}
          </Text>
        </Group>
      </UnstyledButton>
      <Collapse in={opened}>
        <Stack gap="xs" mt="xs">
          {evidence.map((item) => (
            <Group key={item.id} gap="xs" align="flex-start" wrap="nowrap">
              <Text
                size="sm"
                fw={700}
                c={item.stance === 'pro' ? 'green' : 'red'}
                style={{ flexShrink: 0, lineHeight: 1.5 }}
              >
                {item.stance === 'pro' ? '+' : '−'}
              </Text>
              <div>
                <Text size="sm" style={{ display: 'inline' }}>
                  {item.claim}
                </Text>
                {item.source ? (
                  <Text size="xs" c="dimmed" style={{ display: 'inline' }}>
                    {' '}
                    — {item.source}
                  </Text>
                ) : null}
              </div>
            </Group>
          ))}
        </Stack>
      </Collapse>
    </div>
  );
}
