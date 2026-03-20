/**
 * BotTraceViewer — collapsible timeline of bot trace steps for a poll
 */

import { useState } from 'react';
import { IconChevronDown, IconChevronUp, IconFlask } from '@tabler/icons-react';
import { Badge, Card, Collapse, Group, Stack, Text, Timeline, UnstyledButton } from '@mantine/core';
import type { BotTrace } from '../api';

interface BotTraceViewerProps {
  traces: BotTrace[];
}

export function BotTraceViewer({ traces }: BotTraceViewerProps) {
  if (traces.length === 0) {
    return null;
  }

  return (
    <Stack gap="sm">
      {traces.map((trace) => (
        <TraceCard key={trace.id} trace={trace} />
      ))}
    </Stack>
  );
}

function TraceCard({ trace }: { trace: BotTrace }) {
  const [opened, setOpened] = useState(false);

  const statusColor =
    trace.status === 'completed' ? 'green' : trace.status === 'failed' ? 'red' : 'yellow';

  return (
    <Card shadow="sm" padding="md" radius="md" withBorder>
      <UnstyledButton
        onClick={() => {
          setOpened((o) => !o);
        }}
        w="100%"
      >
        <Group justify="space-between" wrap="nowrap">
          <Group gap="xs" wrap="nowrap">
            <IconFlask size={16} color="var(--mantine-color-blue-6)" />
            <Text size="sm" fw={500}>
              Bot generated
            </Text>
            <Text size="sm" c="dimmed">
              · {String(trace.steps.length)} step{trace.steps.length !== 1 ? 's' : ''}
            </Text>
            <Text size="sm" c="dimmed">
              · ${trace.total_cost_usd.toFixed(3)}
            </Text>
          </Group>
          <Group gap="xs" wrap="nowrap">
            <Badge color={statusColor} variant="light" size="sm">
              {trace.status}
            </Badge>
            {opened ? <IconChevronUp size={16} /> : <IconChevronDown size={16} />}
          </Group>
        </Group>
      </UnstyledButton>

      <Collapse in={opened}>
        <Stack gap="md" mt="md">
          {trace.error ? (
            <Text size="sm" c="red">
              Error: {trace.error}
            </Text>
          ) : null}

          <Timeline active={trace.steps.length - 1} bulletSize={20} lineWidth={2}>
            {trace.steps.map((step, i) => (
              <Timeline.Item
                key={i}
                title={
                  <Group gap="xs" wrap="nowrap">
                    <Text size="sm" fw={500}>
                      {step.type}
                    </Text>
                    {step.model ? (
                      <Badge variant="outline" size="xs" color="blue">
                        {step.model}
                      </Badge>
                    ) : null}
                    {Object.keys(step.cache).length > 0 ? (
                      <Badge variant="outline" size="xs" color="grape">
                        cached
                      </Badge>
                    ) : null}
                  </Group>
                }
              >
                <Stack gap={4} mt={4}>
                  {step.output_summary ? (
                    <Text size="xs" c="dimmed">
                      {step.output_summary}
                    </Text>
                  ) : null}
                  <Group gap="md">
                    {step.prompt_tokens != null || step.completion_tokens != null ? (
                      <Text size="xs" c="dimmed">
                        {step.prompt_tokens != null ? String(step.prompt_tokens) : '?'}
                        {' + '}
                        {step.completion_tokens != null ? String(step.completion_tokens) : '?'}{' '}
                        tokens
                      </Text>
                    ) : null}
                    <Text size="xs" c="dimmed">
                      ${step.cost_usd.toFixed(4)}
                    </Text>
                    <Text size="xs" c="dimmed">
                      {String(step.latency_ms)}ms
                    </Text>
                  </Group>
                </Stack>
              </Timeline.Item>
            ))}
          </Timeline>
        </Stack>
      </Collapse>
    </Card>
  );
}
