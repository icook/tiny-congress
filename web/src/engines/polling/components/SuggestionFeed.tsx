/**
 * SuggestionFeed — lets authenticated users suggest research topics for a room,
 * and shows all suggestions with their current processing status.
 */

import { useState } from 'react';
import { IconBulb, IconSend } from '@tabler/icons-react';
import { Badge, Button, Card, Group, Stack, Text, TextInput, Title } from '@mantine/core';
import { useCrypto } from '@/providers/CryptoProvider';
import { useDevice } from '@/providers/DeviceProvider';
import { useCreateSuggestion, useSuggestions, type Suggestion } from '../api';

const STATUS_COLOR: Record<string, string> = {
  queued: 'yellow',
  researching: 'blue',
  completed: 'green',
  rejected: 'red',
  failed: 'red',
};

interface SuggestionFeedProps {
  roomId: string;
  pollId: string;
}

export function SuggestionFeed({ roomId, pollId }: SuggestionFeedProps) {
  const { deviceKid, privateKey } = useDevice();
  const { crypto } = useCrypto();
  const suggestionsQuery = useSuggestions(roomId, pollId);
  const createMutation = useCreateSuggestion(roomId, pollId, deviceKid, privateKey, crypto);
  const [text, setText] = useState('');

  const handleSubmit = () => {
    const trimmed = text.trim();
    if (!trimmed) {
      return;
    }
    createMutation.mutate(trimmed, {
      onSuccess: () => {
        setText('');
      },
    });
  };

  const isAuthenticated = Boolean(deviceKid && privateKey && crypto);

  return (
    <Stack gap="sm">
      <Group gap="xs">
        <IconBulb size={20} />
        <Title order={4}>Research Suggestions</Title>
      </Group>

      {isAuthenticated ? (
        <Group gap="xs" align="end">
          <TextInput
            placeholder="Suggest something to investigate..."
            value={text}
            onChange={(e) => {
              setText(e.currentTarget.value);
            }}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                handleSubmit();
              }
            }}
            style={{ flex: 1 }}
            maxLength={500}
            error={createMutation.error?.message}
          />
          <Button
            onClick={handleSubmit}
            loading={createMutation.isPending}
            leftSection={<IconSend size={16} />}
            size="sm"
          >
            Suggest
          </Button>
        </Group>
      ) : null}

      {suggestionsQuery.data?.length === 0 ? (
        <Text size="sm" c="dimmed">
          No suggestions yet — be the first to steer the research.
        </Text>
      ) : null}

      {suggestionsQuery.data?.map((s) => (
        <SuggestionItem key={s.id} suggestion={s} />
      ))}
    </Stack>
  );
}

function SuggestionItem({ suggestion }: { suggestion: Suggestion }) {
  const color = STATUS_COLOR[suggestion.status] ?? 'gray';

  return (
    <Card padding="sm" radius="sm" withBorder>
      <Group justify="space-between" wrap="nowrap">
        <Text size="sm">{suggestion.suggestion_text}</Text>
        <Badge color={color} variant="light" size="sm">
          {suggestion.status}
        </Badge>
      </Group>
      {suggestion.status === 'rejected' && suggestion.filter_reason ? (
        <Text size="xs" c="dimmed" mt={4}>
          {suggestion.filter_reason}
        </Text>
      ) : null}
    </Card>
  );
}
