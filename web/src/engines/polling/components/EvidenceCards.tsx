/**
 * EvidenceCards — collapsible list of pro/con evidence items for a dimension
 */

import { Collapse, Group, Stack, Text, UnstyledButton } from '@mantine/core';
import { useDisclosure } from '@mantine/hooks';
import type { Evidence } from '../api';

interface EvidenceCardsProps {
  evidence: Evidence[];
}

export function EvidenceCards({ evidence }: EvidenceCardsProps) {
  const [opened, { toggle }] = useDisclosure(false);

  if (evidence.length === 0) {
    return null;
  }

  return (
    <div>
      <UnstyledButton onClick={toggle}>
        <Text size="xs" c="dimmed" style={{ cursor: 'pointer' }}>
          {String(evidence.length)} evidence card{evidence.length !== 1 ? 's' : ''}{' '}
          {opened ? '▲' : '▼'}
        </Text>
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
