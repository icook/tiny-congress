/**
 * Poll page - Vote on dimensions with sliders, view results
 */

import { useCallback, useEffect, useState } from 'react';
import { IconAlertTriangle, IconCheck, IconLock } from '@tabler/icons-react';
import { Link } from '@tanstack/react-router';
import {
  Alert,
  Badge,
  Button,
  Card,
  Group,
  Loader,
  Progress,
  Slider,
  Stack,
  Table,
  Text,
  Title,
} from '@mantine/core';
import {
  useCastVote,
  useMyVotes,
  usePollDetail,
  usePollResults,
  type Dimension,
  type DimensionVote,
} from '@/features/rooms';
import { useCrypto } from '@/providers/CryptoProvider';
import { useDevice } from '@/providers/DeviceProvider';

interface PollPageProps {
  roomId: string;
  pollId: string;
}

export function PollPage({ roomId, pollId }: PollPageProps) {
  const { deviceKid, privateKey, isLoading: deviceLoading } = useDevice();
  const { crypto } = useCrypto();

  const detailQuery = usePollDetail(roomId, pollId);
  const resultsQuery = usePollResults(roomId, pollId);
  const myVotesQuery = useMyVotes(roomId, pollId, deviceKid, privateKey, crypto);
  const voteMutation = useCastVote(roomId, pollId, deviceKid, privateKey, crypto);

  const [votes, setVotes] = useState<Record<string, number>>({});
  const [hasInitialized, setHasInitialized] = useState(false);

  // Initialize slider values from existing votes or dimension midpoints
  useEffect(() => {
    if (hasInitialized || !detailQuery.data) {
      return;
    }

    const initial: Record<string, number> = {};
    for (const dim of detailQuery.data.dimensions) {
      const existingVote = myVotesQuery.data?.find((v) => v.dimension_id === dim.id);
      initial[dim.id] = existingVote ? existingVote.value : (dim.min_value + dim.max_value) / 2;
    }
    setVotes(initial);
    setHasInitialized(true);
  }, [detailQuery.data, myVotesQuery.data, hasInitialized]);

  const handleVoteChange = useCallback((dimensionId: string, value: number) => {
    setVotes((prev) => ({ ...prev, [dimensionId]: value }));
  }, []);

  const handleSubmit = useCallback(() => {
    if (!detailQuery.data) {
      return;
    }
    const dimensionVotes: DimensionVote[] = detailQuery.data.dimensions.map((dim) => ({
      dimension_id: dim.id,
      value: votes[dim.id] ?? (dim.min_value + dim.max_value) / 2,
    }));
    voteMutation.mutate(dimensionVotes);
  }, [detailQuery.data, votes, voteMutation]);

  if (detailQuery.isLoading || deviceLoading) {
    return (
      <Stack gap="md" maw={800} mx="auto" mt="xl">
        <Loader size="sm" />
      </Stack>
    );
  }

  if (detailQuery.isError) {
    return (
      <Stack gap="md" maw={800} mx="auto" mt="xl">
        <Alert icon={<IconAlertTriangle size={16} />} color="red">
          Failed to load poll: {detailQuery.error.message}
        </Alert>
      </Stack>
    );
  }

  if (!detailQuery.data) {
    return null;
  }

  const { poll, dimensions } = detailQuery.data;
  const isActive = poll.status === 'active';
  const isAuthenticated = Boolean(deviceKid);
  const canVote = isActive && isAuthenticated;
  const hasVoted = (myVotesQuery.data?.length ?? 0) > 0;

  return (
    <Stack gap="md" maw={800} mx="auto" mt="xl">
      <Group>
        <Text component={Link} to="/rooms" size="sm" c="dimmed" style={{ textDecoration: 'none' }}>
          Rooms
        </Text>
        <Text size="sm" c="dimmed">
          /
        </Text>
      </Group>

      <div>
        <Group justify="space-between" align="flex-start">
          <Title order={2}>{poll.question}</Title>
          <Badge
            color={isActive ? 'blue' : poll.status === 'closed' ? 'gray' : 'yellow'}
            variant="light"
          >
            {poll.status}
          </Badge>
        </Group>
        {poll.description ? (
          <Text c="dimmed" size="sm" mt="xs">
            {poll.description}
          </Text>
        ) : null}
      </div>

      {/* Voting section */}
      {isActive ? (
        <Card shadow="sm" padding="lg" radius="md" withBorder>
          <Stack gap="md">
            <Title order={4}>Cast Your Vote</Title>

            {!isAuthenticated ? (
              <Alert icon={<IconLock size={16} />} color="yellow">
                Sign up or log in to vote on this poll.
              </Alert>
            ) : null}

            {voteMutation.isError ? (
              <Alert icon={<IconAlertTriangle size={16} />} color="red">
                {voteMutation.error.message}
              </Alert>
            ) : null}

            {voteMutation.isSuccess ? (
              <Alert icon={<IconCheck size={16} />} color="green">
                Vote submitted! See the results below.
              </Alert>
            ) : null}

            {dimensions.map((dim) => (
              <VoteSlider
                key={dim.id}
                dimension={dim}
                value={votes[dim.id] ?? (dim.min_value + dim.max_value) / 2}
                onChange={(v) => {
                  handleVoteChange(dim.id, v);
                }}
                disabled={!canVote}
              />
            ))}

            {canVote ? (
              <Button onClick={handleSubmit} loading={voteMutation.isPending} fullWidth>
                {hasVoted ? 'Update Vote' : 'Submit Vote'}
              </Button>
            ) : null}
          </Stack>
        </Card>
      ) : null}

      {/* Results section */}
      <Card shadow="sm" padding="lg" radius="md" withBorder>
        <Stack gap="md">
          <Group justify="space-between">
            <Title order={4}>Results</Title>
            {resultsQuery.data ? (
              <Text size="sm" c="dimmed">
                {String(resultsQuery.data.voter_count)} voter
                {resultsQuery.data.voter_count !== 1 ? 's' : ''}
              </Text>
            ) : null}
          </Group>

          {resultsQuery.isLoading ? <Loader size="sm" /> : null}

          {resultsQuery.isError ? (
            <Alert icon={<IconAlertTriangle size={16} />} color="red">
              Failed to load results: {resultsQuery.error.message}
            </Alert>
          ) : null}

          {resultsQuery.data?.voter_count === 0 ? (
            <Text size="sm" c="dimmed">
              No votes yet.
            </Text>
          ) : null}

          {(resultsQuery.data?.voter_count ?? 0) > 0 ? (
            <ResultsTable dimensions={resultsQuery.data?.dimensions ?? []} />
          ) : null}
        </Stack>
      </Card>
    </Stack>
  );
}

function VoteSlider({
  dimension,
  value,
  onChange,
  disabled,
}: {
  dimension: Dimension;
  value: number;
  onChange: (value: number) => void;
  disabled: boolean;
}) {
  return (
    <div>
      <Group justify="space-between" mb="xs">
        <Text size="sm" fw={500}>
          {dimension.name}
        </Text>
        <Text size="sm" c="dimmed">
          {value.toFixed(2)}
        </Text>
      </Group>
      {dimension.description ? (
        <Text size="xs" c="dimmed" mb="xs">
          {dimension.description}
        </Text>
      ) : null}
      <Slider
        value={value}
        onChange={onChange}
        min={dimension.min_value}
        max={dimension.max_value}
        step={0.01}
        disabled={disabled}
        label={(val) => {
          const pct = Math.round(
            ((val - dimension.min_value) / (dimension.max_value - dimension.min_value)) * 100
          );
          return `${String(pct)}%`;
        }}
        marks={[
          { value: dimension.min_value, label: 'Not at all' },
          { value: dimension.max_value, label: 'Extremely' },
        ]}
      />
    </div>
  );
}

function ResultsTable({
  dimensions,
}: {
  dimensions: {
    dimension_name: string;
    count: number;
    mean: number;
    median: number;
    stddev: number;
    min: number;
    max: number;
  }[];
}) {
  return (
    <Stack gap="md">
      {dimensions.map((dim) => {
        const range = dim.max - dim.min;
        const pct = range > 0 ? ((dim.mean - dim.min) / range) * 100 : 0;
        return (
          <div key={dim.dimension_name}>
            <Group justify="space-between" mb={4}>
              <Text size="sm" fw={500}>
                {dim.dimension_name}
              </Text>
              <Text size="xs" c="dimmed">
                mean: {dim.mean.toFixed(2)} | median: {dim.median.toFixed(2)} | votes:{' '}
                {String(dim.count)}
              </Text>
            </Group>
            <Progress value={pct} size="lg" radius="sm" />
          </div>
        );
      })}

      <Table striped highlightOnHover withTableBorder>
        <Table.Thead>
          <Table.Tr>
            <Table.Th>Dimension</Table.Th>
            <Table.Th ta="right">Mean</Table.Th>
            <Table.Th ta="right">Median</Table.Th>
            <Table.Th ta="right">Std Dev</Table.Th>
            <Table.Th ta="right">Votes</Table.Th>
          </Table.Tr>
        </Table.Thead>
        <Table.Tbody>
          {dimensions.map((dim) => (
            <Table.Tr key={dim.dimension_name}>
              <Table.Td>{dim.dimension_name}</Table.Td>
              <Table.Td ta="right">{dim.mean.toFixed(3)}</Table.Td>
              <Table.Td ta="right">{dim.median.toFixed(3)}</Table.Td>
              <Table.Td ta="right">{dim.stddev.toFixed(3)}</Table.Td>
              <Table.Td ta="right">{String(dim.count)}</Table.Td>
            </Table.Tr>
          ))}
        </Table.Tbody>
      </Table>
    </Stack>
  );
}
