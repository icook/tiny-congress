/**
 * Developer docs — Architecture overview
 */

import { IconArrowLeft } from '@tabler/icons-react';
import { Link } from '@tanstack/react-router';
import { Anchor, Group, Stack, Title } from '@mantine/core';
import { MarkdownContent } from '../components/MarkdownContent';
import architectureContent from '../content/dev/architecture.md?raw';

export function DevArchitecturePage() {
  return (
    <Stack gap="md" maw={780} mx="auto">
      <Group gap="xs">
        <Anchor component={Link} to="/dev" size="sm" c="dimmed">
          <Group gap={4}>
            <IconArrowLeft size={14} />
            Dev Docs
          </Group>
        </Anchor>
      </Group>

      <Title order={2}>Architecture Overview</Title>

      <MarkdownContent>{architectureContent}</MarkdownContent>
    </Stack>
  );
}
