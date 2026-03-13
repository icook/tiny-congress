/**
 * Developer documentation index — lists available technical reference pages.
 */

import { IconBook2, IconCode } from '@tabler/icons-react';
import { Link } from '@tanstack/react-router';
import { Anchor, Badge, Card, Group, SimpleGrid, Stack, Text, Title } from '@mantine/core';

const docs = [
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
] as const;

export function DevIndexPage() {
  return (
    <Stack gap="md">
      <Group gap="xs">
        <IconCode size={20} />
        <Title order={2}>Developer Documentation</Title>
        <Badge variant="light" color="gray" size="sm">
          For contributors
        </Badge>
      </Group>

      <Text c="dimmed" size="sm">
        Technical reference for developers building on or contributing to TinyCongress. For end-user
        guides, see{' '}
        <Anchor component={Link} to="/about">
          About
        </Anchor>
        .
      </Text>

      <SimpleGrid cols={{ base: 1, sm: 2 }} spacing="md">
        {docs.map((doc) => (
          <Card
            key={doc.path}
            component={Link}
            to={doc.path}
            shadow="sm"
            padding="lg"
            radius="md"
            withBorder
            style={{ textDecoration: 'none', cursor: 'pointer' }}
          >
            <Group gap="xs" mb="xs">
              <IconBook2 size={16} />
              <Text fw={500}>{doc.title}</Text>
            </Group>
            <Text size="sm" c="dimmed">
              {doc.description}
            </Text>
          </Card>
        ))}
      </SimpleGrid>
    </Stack>
  );
}
