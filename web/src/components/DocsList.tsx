import { IconBook2 } from '@tabler/icons-react';
import { Link } from '@tanstack/react-router';
import { Card, Group, Stack, Text } from '@mantine/core';

export interface DocEntry {
  title: string;
  description: string;
  path: string;
}

interface DocsListProps {
  docs: readonly DocEntry[];
}

/** Vertical list of doc link cards — shared between docs index and dev docs index. */
export function DocsList({ docs }: DocsListProps) {
  return (
    <Stack gap="sm">
      {docs.map((doc) => (
        <Card
          key={doc.path}
          component={Link}
          to={doc.path}
          shadow="sm"
          padding="md"
          radius="md"
          withBorder
          td="none"
          style={{ cursor: 'pointer' }}
        >
          <Group gap="xs" wrap="nowrap">
            <IconBook2 size={16} style={{ flexShrink: 0 }} />
            <div>
              <Text fw={500} size="sm">
                {doc.title}
              </Text>
              <Text size="xs" c="dimmed">
                {doc.description}
              </Text>
            </div>
          </Group>
        </Card>
      ))}
    </Stack>
  );
}
