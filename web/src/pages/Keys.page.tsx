/**
 * Keys page - Plain-language explainer of how TinyCongress keys work
 */

import { IconKey } from '@tabler/icons-react';
import { Group, Stack, Title } from '@mantine/core';
import { MarkdownContent } from '../components/MarkdownContent';
import keysContent from '../content/keys.md?raw';

export function KeysPage() {
  return (
    <Stack gap="md" maw={780} mx="auto">
      <Group gap="xs">
        <IconKey size={20} />
        <Title order={2}>How Your Keys Work</Title>
      </Group>

      <MarkdownContent>{keysContent}</MarkdownContent>
    </Stack>
  );
}
