/**
 * Developer documentation index — lists available technical reference pages.
 */

import { IconBrandGithub, IconCode } from '@tabler/icons-react';
import { Anchor, Badge, Group, Stack, Text, Title } from '@mantine/core';
import { DocsList, type DocEntry } from '../components/DocsList';

const docs: readonly DocEntry[] = [
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

export function DevIndexPage() {
  return (
    <Stack gap="md" maw={780} mx="auto">
      <Group gap="xs">
        <IconCode size={20} />
        <Title order={2}>Developer Documentation</Title>
        <Badge variant="light" color="gray" size="sm">
          For contributors
        </Badge>
      </Group>

      <Text c="dimmed" size="sm">
        Technical reference for developers building on or contributing to TinyCongress.{' '}
        <Anchor href="https://github.com/icook/tiny-congress" target="_blank" rel="noopener">
          <IconBrandGithub size={14} style={{ verticalAlign: 'text-bottom' }} /> Source on GitHub
        </Anchor>
      </Text>

      <DocsList docs={docs} />
    </Stack>
  );
}
