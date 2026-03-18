/**
 * Developer docs — Domain model reference
 */

import { IconArrowLeft } from '@tabler/icons-react';
import { Link } from '@tanstack/react-router';
import { Anchor, Group, Stack, Title } from '@mantine/core';
import { MarkdownContent } from '../components/MarkdownContent';
import domainModelContent from '../content/dev/domain-model.md?raw';

export function DevDomainModelPage() {
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

      <Title order={2}>Domain Model</Title>

      <MarkdownContent>{domainModelContent}</MarkdownContent>
    </Stack>
  );
}
