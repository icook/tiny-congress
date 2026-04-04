import { Badge, Center, Image, Loader, Stack, Table, Text, Title } from '@mantine/core';
import { getApiBaseUrl } from '@/config';
import { useLeaderboard, type Submission } from '../api';

interface Props {
  roomId: string;
  roundId: string;
  roundStatus: string;
}

const RANK_COLORS: Record<number, string> = {
  1: 'yellow',
  2: 'gray',
  3: 'orange',
};

function rankLabel(rank: number): string {
  if (rank === 1) {
    return '1st';
  }
  if (rank === 2) {
    return '2nd';
  }
  if (rank === 3) {
    return '3rd';
  }
  return `${String(rank)}th`;
}

function SubmissionPreviewCell({
  submission,
  showAuthor,
}: {
  submission: Submission;
  showAuthor: boolean;
}) {
  return (
    <Stack gap={2}>
      {submission.content_type === 'url' && submission.url ? (
        <Text size="sm" truncate maw={250} c="blue" td="underline">
          {submission.url}
        </Text>
      ) : null}
      {submission.content_type === 'image' && submission.image_key ? (
        <Image
          src={`${getApiBaseUrl()}/api/v1/uploads/${submission.image_key}`}
          alt={submission.caption ?? 'Submission'}
          maw={120}
          radius="sm"
          fallbackSrc="data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7"
        />
      ) : null}
      {submission.caption ? (
        <Text size="xs" c="dimmed" truncate maw={250}>
          {submission.caption}
        </Text>
      ) : null}
      {showAuthor && submission.author_id ? (
        <Text size="xs" c="dimmed">
          by {submission.author_id}
        </Text>
      ) : null}
    </Stack>
  );
}

export function Leaderboard({ roomId, roundId, roundStatus }: Props) {
  const { data, isLoading, isError, error } = useLeaderboard(roomId, roundId);

  const showAuthor = roundStatus === 'closed';

  if (isLoading) {
    return (
      <Center mt="xl">
        <Loader size="sm" />
      </Center>
    );
  }

  if (isError) {
    return (
      <Text size="sm" c="red" mt="md">
        {error.message}
      </Text>
    );
  }

  if (!data || data.entries.length === 0) {
    return (
      <Text size="sm" c="dimmed" mt="md">
        No submissions yet.
      </Text>
    );
  }

  return (
    <Stack gap="md" mt="md">
      <Title order={4}>Leaderboard</Title>
      {roundStatus === 'ranking' ? (
        <Text size="xs" c="dimmed">
          Authors are hidden during the ranking phase.
        </Text>
      ) : null}
      <Table striped highlightOnHover>
        <Table.Thead>
          <Table.Tr>
            <Table.Th>Rank</Table.Th>
            <Table.Th>Submission</Table.Th>
            <Table.Th>Rating</Table.Th>
            <Table.Th>Matchups</Table.Th>
          </Table.Tr>
        </Table.Thead>
        <Table.Tbody>
          {data.entries.map((entry) => (
            <Table.Tr key={entry.submission.id}>
              <Table.Td>
                <Badge
                  color={RANK_COLORS[entry.rank] ?? 'blue'}
                  variant={entry.rank <= 3 ? 'filled' : 'light'}
                  size="sm"
                >
                  {rankLabel(entry.rank)}
                </Badge>
              </Table.Td>
              <Table.Td>
                <SubmissionPreviewCell submission={entry.submission} showAuthor={showAuthor} />
              </Table.Td>
              <Table.Td>
                <Text size="sm">{entry.rating.toFixed(0)}</Text>
              </Table.Td>
              <Table.Td>
                <Text size="sm">{String(entry.matchup_count)}</Text>
              </Table.Td>
            </Table.Tr>
          ))}
        </Table.Tbody>
      </Table>
    </Stack>
  );
}
