/**
 * BotTraceViewer — three-level progressive disclosure of bot trace data
 *
 * Level 1 (collapsed): "Bot generated . 4 steps . $0.012 . 2m ago"
 * Level 2 (expanded): timeline of each step with model, tokens, cost, cache
 * Level 3 (full detail): per-step output summary in expandable blocks
 */

import { useState } from 'react';
import {
  IconChevronDown,
  IconChevronUp,
  IconFlask,
  IconRobot,
  IconSearch,
} from '@tabler/icons-react';
import {
  Badge,
  Card,
  Code,
  Collapse,
  Group,
  Stack,
  Text,
  Timeline,
  Title,
  UnstyledButton,
} from '@mantine/core';
import type { BotTrace, TraceStep } from '../api';

// ─── Helpers ────────────────────────────────────────────────────────────────

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

function timeAgo(isoString: string): string {
  const diff = new Date(isoString).getTime() - Date.now();
  for (const [unit, ms] of UNITS) {
    if (Math.abs(diff) >= ms) {
      return rtf.format(Math.round(diff / ms), unit);
    }
  }
  return rtf.format(0, 'second');
}

/** Returns true when at least one cache layer reports a hit. */
function hasCacheHit(cache: Record<string, unknown>): boolean {
  return Object.values(cache).some((v) => Boolean(v));
}

const STEP_TYPE_LABELS: Record<string, string> = {
  llm_call: 'LLM Call',
  llm_synthesis: 'LLM Synthesis',
  exa_search: 'Web Search',
};

function stepTypeLabel(stepType: string): string {
  if (STEP_TYPE_LABELS[stepType]) {
    return STEP_TYPE_LABELS[stepType];
  }
  return stepType
    .split('_')
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(' ');
}

/** Short model name: strip provider prefix (e.g. "anthropic/claude-sonnet-4-20250514" -> "claude-sonnet-4-20250514") */
function shortModel(model: string): string {
  const slash = model.lastIndexOf('/');
  return slash >= 0 ? model.slice(slash + 1) : model;
}

function stepIcon(stepType: string) {
  if (stepType === 'exa_search') {
    return <IconSearch size={12} />;
  }
  return <IconRobot size={12} />;
}

// ─── Components ─────────────────────────────────────────────────────────────

interface BotTraceViewerProps {
  traces: BotTrace[];
}

export function BotTraceViewer({ traces }: BotTraceViewerProps) {
  if (traces.length === 0) {
    return null;
  }

  return (
    <Card shadow="sm" padding="lg" radius="md" withBorder>
      <Stack gap="md">
        <Group gap="xs">
          <IconFlask size={18} color="var(--mantine-color-blue-6)" />
          <Title order={4}>Research Trace</Title>
        </Group>
        <Stack gap="sm">
          {traces.map((trace) => (
            <TraceCard key={trace.id} trace={trace} />
          ))}
        </Stack>
      </Stack>
    </Card>
  );
}

function TraceCard({ trace }: { trace: BotTrace }) {
  const [opened, setOpened] = useState(false);

  const statusColor =
    trace.status === 'completed' ? 'green' : trace.status === 'failed' ? 'red' : 'yellow';

  const timestampSource = trace.completed_at ?? trace.created_at;

  return (
    <Card padding="sm" radius="sm" withBorder>
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
            <Text size="sm" c="dimmed">
              · {timeAgo(timestampSource)}
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
                bullet={stepIcon(step.type)}
                title={
                  <Group gap="xs" wrap="nowrap">
                    <Text size="sm" fw={500}>
                      {stepTypeLabel(step.type)}
                    </Text>
                    {step.model ? (
                      <Badge variant="outline" size="xs" color="blue">
                        {shortModel(step.model)}
                      </Badge>
                    ) : null}
                    {hasCacheHit(step.cache) ? (
                      <Badge variant="light" size="xs" color="grape">
                        cached
                      </Badge>
                    ) : null}
                  </Group>
                }
              >
                <StepDetail step={step} />
              </Timeline.Item>
            ))}
          </Timeline>
        </Stack>
      </Collapse>
    </Card>
  );
}

function StepDetail({ step }: { step: TraceStep }) {
  const [showFull, setShowFull] = useState(false);

  return (
    <Stack gap={4} mt={4}>
      {step.query ? (
        <Text size="xs" c="dimmed" fs="italic">
          &ldquo;{step.query}&rdquo;
        </Text>
      ) : null}

      <Group gap="md">
        {step.prompt_tokens != null || step.completion_tokens != null ? (
          <Text size="xs" c="dimmed">
            {step.prompt_tokens != null ? String(step.prompt_tokens) : '?'}
            {' + '}
            {step.completion_tokens != null ? String(step.completion_tokens) : '?'} tokens
          </Text>
        ) : null}
        <Text size="xs" c="dimmed">
          ${step.cost_usd.toFixed(4)}
        </Text>
        <Text size="xs" c="dimmed">
          {String(step.latency_ms)}ms
        </Text>
      </Group>

      {step.output_summary ? (
        <>
          {!showFull ? (
            <UnstyledButton
              onClick={() => {
                setShowFull(true);
              }}
            >
              <Text size="xs" c="blue" td="underline">
                Show output
              </Text>
            </UnstyledButton>
          ) : (
            <>
              <UnstyledButton
                onClick={() => {
                  setShowFull(false);
                }}
              >
                <Text size="xs" c="blue" td="underline">
                  Hide output
                </Text>
              </UnstyledButton>
              <Code block style={{ whiteSpace: 'pre-wrap', maxHeight: 300, overflow: 'auto' }}>
                {step.output_summary}
              </Code>
            </>
          )}
        </>
      ) : null}
    </Stack>
  );
}
