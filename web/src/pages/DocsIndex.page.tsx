/**
 * Documentation index — lists all user-facing and developer reference pages.
 */

import { IconBook2, IconBrandGithub, IconCode, IconInfoCircle } from '@tabler/icons-react';
import { Anchor, Group, Stack, Text, Title } from '@mantine/core';
import { DocsList, type DocEntry } from '../components/DocsList';

const guideDocs: readonly DocEntry[] = [
  {
    title: 'About TinyCongress',
    description: 'What the platform does, how voting works, and why keys matter.',
    path: '/about',
  },
  {
    title: 'How Your Keys Work',
    description: 'Plain-language explainer of root keys, device keys, and backup envelopes.',
    path: '/keys',
  },
];

const devDocs: readonly DocEntry[] = [
  {
    title: 'Architecture Overview',
    description: 'System components, request flow, background workers, and how the pieces connect.',
    path: '/dev/architecture',
  },
  {
    title: 'Domain Model',
    description:
      'Core entities, data invariants, trust boundaries, and the signup/login flows in detail.',
    path: '/dev/domain-model',
  },
];

export function DocsIndexPage() {
  return (
    <Stack gap="lg">
      <Group gap="xs">
        <IconBook2 size={20} />
        <Title order={2}>Documentation</Title>
      </Group>

      <section>
        <Group gap="xs" mb="xs">
          <IconInfoCircle size={16} />
          <Title order={4}>Guides</Title>
        </Group>
        <DocsList docs={guideDocs} />
      </section>

      <section>
        <Group gap="xs" mb="xs">
          <IconCode size={16} />
          <Title order={4}>Developer Reference</Title>
        </Group>
        <Text c="dimmed" size="xs" mb="xs">
          Technical docs for contributors.{' '}
          <Anchor href="https://github.com/icook/tiny-congress" target="_blank" rel="noopener">
            <IconBrandGithub size={12} style={{ verticalAlign: 'text-bottom' }} /> Source on GitHub
          </Anchor>
        </Text>
        <DocsList docs={devDocs} />
      </section>
    </Stack>
  );
}
