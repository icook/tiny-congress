import { useState } from 'react';
import {
  Badge,
  Button,
  Card,
  Center,
  Group,
  Image,
  Loader,
  Stack,
  Text,
  Title,
} from '@mantine/core';
import { getApiBaseUrl } from '@/config';
import { getHallOfFame, type HallOfFameEntry } from '../api';

const PAGE_SIZE = 20;

interface Props {
  roomId: string;
}

function HallOfFameCard({ entry }: { entry: HallOfFameEntry }) {
  return (
    <Card withBorder padding="sm" radius="sm">
      <Group justify="space-between" wrap="nowrap" align="flex-start">
        <Stack gap={4}>
          <Group gap="xs" wrap="nowrap">
            <Badge color="yellow" variant="filled" size="sm">
              Round {String(entry.round_number)}
            </Badge>
            <Badge color="blue" variant="light" size="sm">
              #{String(entry.rank)}
            </Badge>
          </Group>
          {entry.submission.content_type === 'url' && entry.submission.url ? (
            <Text size="sm" truncate maw={300} c="blue" td="underline">
              {entry.submission.url}
            </Text>
          ) : null}
          {entry.submission.content_type === 'image' && entry.submission.image_key ? (
            <Image
              src={`${getApiBaseUrl()}/api/v1/uploads/${entry.submission.image_key}`}
              alt={entry.submission.caption ?? 'Submission'}
              maw={200}
              radius="sm"
              fallbackSrc="data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7"
            />
          ) : null}
          {entry.submission.caption ? <Text size="sm">{entry.submission.caption}</Text> : null}
          <Text size="xs" c="dimmed">
            by {entry.submission.author_id}
          </Text>
        </Stack>
        <Stack gap={2} align="flex-end">
          <Text size="xs" c="dimmed">
            Rating
          </Text>
          <Text size="sm" fw={600}>
            {entry.final_rating.toFixed(0)}
          </Text>
        </Stack>
      </Group>
    </Card>
  );
}

type LoadState = 'idle' | 'loading' | 'error';

export function HallOfFame({ roomId }: Props) {
  const [entries, setEntries] = useState<HallOfFameEntry[]>([]);
  const [offset, setOffset] = useState(0);
  const [initialState, setInitialState] = useState<LoadState>('loading');
  const [isLoadingMore, setIsLoadingMore] = useState(false);
  const [hasMore, setHasMore] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Initial load — useEffect would be cleaner but this component manages its own
  // pagination state outside the query cache, so we use a ref-guarded one-shot.
  const [initialized, setInitialized] = useState(false);
  if (!initialized) {
    setInitialized(true);
    getHallOfFame(roomId, PAGE_SIZE, 0)
      .then((data) => {
        setEntries(data);
        setHasMore(data.length === PAGE_SIZE);
        setOffset(data.length);
        setInitialState('idle');
      })
      .catch((err: unknown) => {
        setError(err instanceof Error ? err.message : 'Failed to load Hall of Fame');
        setInitialState('error');
      });
  }

  const handleLoadMore = () => {
    setIsLoadingMore(true);
    getHallOfFame(roomId, PAGE_SIZE, offset)
      .then((data) => {
        setEntries((prev) => [...prev, ...data]);
        setHasMore(data.length === PAGE_SIZE);
        setOffset((prev) => prev + data.length);
        setIsLoadingMore(false);
      })
      .catch((err: unknown) => {
        setError(err instanceof Error ? err.message : 'Failed to load more');
        setIsLoadingMore(false);
      });
  };

  if (initialState === 'loading') {
    return (
      <Center mt="xl">
        <Loader size="sm" />
      </Center>
    );
  }

  if (initialState === 'error') {
    return (
      <Text size="sm" c="red" mt="md">
        {error}
      </Text>
    );
  }

  if (entries.length === 0) {
    return (
      <Text size="sm" c="dimmed" mt="md">
        No winners yet — complete a round to see the Hall of Fame.
      </Text>
    );
  }

  return (
    <Stack gap="md" mt="md">
      <Title order={4}>Hall of Fame</Title>
      {entries.map((entry, i) => (
        <HallOfFameCard
          key={`${String(entry.round_number)}-${String(entry.rank)}-${String(i)}`}
          entry={entry}
        />
      ))}
      {hasMore ? (
        <Button
          variant="subtle"
          onClick={handleLoadMore}
          loading={isLoadingMore}
          size="sm"
          mx="auto"
        >
          Load more
        </Button>
      ) : null}
    </Stack>
  );
}
