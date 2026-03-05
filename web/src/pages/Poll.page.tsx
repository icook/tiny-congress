/**
 * Poll page - Vote on dimensions with sliders, view results
 */

import { useCallback, useEffect, useState } from 'react';
import { IconAlertTriangle, IconCheck, IconLock, IconShieldOff } from '@tabler/icons-react';
import { Link } from '@tanstack/react-router';
import { BarChart } from '@mantine/charts';
import {
  Alert,
  Badge,
  Button,
  Card,
  Group,
  Loader,
  Slider,
  Stack,
  Text,
  Title,
} from '@mantine/core';
import {
  useCastVote,
  useMyVotes,
  usePollDetail,
  usePollDistribution,
  usePollResults,
  type Dimension,
  type DimensionVote,
} from '@/features/rooms';
import { buildVerifierUrl, useVerificationStatus } from '@/features/verification';
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
  const distributionQuery = usePollDistribution(roomId, pollId);
  const myVotesQuery = useMyVotes(roomId, pollId, deviceKid, privateKey, crypto);
  const voteMutation = useCastVote(roomId, pollId, deviceKid, privateKey, crypto);
  const verificationQuery = useVerificationStatus(deviceKid, privateKey, crypto);
  const isVerified = verificationQuery.data?.isVerified ?? false;

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
  const canVote = isActive && isAuthenticated && isVerified;
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

            {isAuthenticated && !isVerified ? (
              <Alert icon={<IconShieldOff size={16} />} color="yellow">
                You need to verify your identity to vote in this room.
                {(() => {
                  const url = buildVerifierUrl('');
                  if (url) {
                    return (
                      <Button component="a" href={url} size="xs" variant="light" mt="xs">
                        Verify Now
                      </Button>
                    );
                  }
                  return null;
                })()}
              </Alert>
            ) : null}

            {voteMutation.isError ? (
              <Alert icon={<IconAlertTriangle size={16} />} color="red">
                {voteMutation.error.message}
              </Alert>
            ) : null}

            {voteMutation.isSuccess ? (
              <Alert icon={<IconCheck size={16} />} color="green">
                Thanks for voting! Scroll down to see what the community thinks.
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
            <Title order={4}>
              {voteMutation.isSuccess ? "Here's what the community thinks:" : 'Results'}
            </Title>
            {resultsQuery.data ? (
              <Text size="sm" c="dimmed">
                {String(resultsQuery.data.voter_count)} voter
                {resultsQuery.data.voter_count !== 1 ? 's' : ''}
              </Text>
            ) : null}
          </Group>

          {distributionQuery.isLoading ? <Loader size="sm" /> : null}

          {distributionQuery.isError ? (
            <Alert icon={<IconAlertTriangle size={16} />} color="red">
              Failed to load results: {distributionQuery.error.message}
            </Alert>
          ) : null}

          {resultsQuery.data?.voter_count === 0 ? (
            <Text size="sm" c="dimmed">
              No votes yet.
            </Text>
          ) : null}

          {(resultsQuery.data?.voter_count ?? 0) > 0 && distributionQuery.data ? (
            <DistributionResults dimensions={distributionQuery.data.dimensions} />
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

function DistributionResults({
  dimensions,
}: {
  dimensions: {
    dimension_id: string;
    dimension_name: string;
    buckets: { label: string; count: number }[];
  }[];
}) {
  return (
    <Stack gap="xl">
      {dimensions.map((dim) => (
        <div key={dim.dimension_id}>
          <Text size="sm" fw={500} mb="xs">
            {dim.dimension_name}
          </Text>
          <BarChart
            h={160}
            data={dim.buckets.map((b) => ({ label: b.label, Votes: b.count }))}
            dataKey="label"
            series={[{ name: 'Votes', color: 'blue.6' }]}
            tickLine="none"
            gridAxis="none"
            withTooltip={false}
            barProps={{ radius: 2 }}
          />
        </div>
      ))}
    </Stack>
  );
}
