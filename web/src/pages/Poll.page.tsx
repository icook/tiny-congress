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
  AgendaProgress,
  EvidenceCards,
  PollCountdown,
  UpcomingPollPreview,
  useAgenda,
  useCastVote,
  useMyVotes,
  usePollCountdown,
  usePollDetail,
  usePollDistribution,
  usePollResults,
  useRoom,
  type Dimension,
  type DimensionVote,
} from '@/features/rooms';
import { useTrustScores } from '@/features/trust';
import { buildVerifierUrl, useVerificationStatus } from '@/features/verification';
import { useCrypto } from '@/providers/CryptoProvider';
import { useDevice } from '@/providers/DeviceProvider';

interface PollPageProps {
  roomId: string;
  pollId: string;
}

export function PollPage({ roomId, pollId }: PollPageProps) {
  const { deviceKid, privateKey, username, isLoading: deviceLoading } = useDevice();
  const { crypto } = useCrypto();

  const roomQuery = useRoom(roomId);
  const detailQuery = usePollDetail(roomId, pollId);
  const resultsQuery = usePollResults(roomId, pollId);
  const distributionQuery = usePollDistribution(roomId, pollId);
  const myVotesQuery = useMyVotes(roomId, pollId, deviceKid, privateKey, crypto);
  const voteMutation = useCastVote(roomId, pollId, deviceKid, privateKey, crypto);
  const verificationQuery = useVerificationStatus(deviceKid, privateKey, crypto);
  const isVerified = verificationQuery.data?.isVerified ?? false;
  const trustScoresQuery = useTrustScores(deviceKid, privateKey, crypto);
  const hasTrustScore = (trustScoresQuery.data?.length ?? 0) > 0;

  const agendaQuery = useAgenda(roomId);
  const { secondsLeft, isExpired } = usePollCountdown(detailQuery.data?.poll);

  // Immediately refetch when countdown expires so the UI transitions without waiting for the next 20s interval
  useEffect(() => {
    if (isExpired) {
      void detailQuery.refetch();
    }
  }, [isExpired]); // eslint-disable-line react-hooks/exhaustive-deps

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

  const hasVoted = (myVotesQuery.data?.length ?? 0) > 0;

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
        <Alert icon={<IconAlertTriangle size={16} />} color="red" title="Poll not found">
          This poll may have been removed or the link may be incorrect.
        </Alert>
        <Button component={Link} to="/rooms" variant="light">
          Browse Rooms
        </Button>
      </Stack>
    );
  }

  if (!detailQuery.data) {
    return null;
  }

  const { poll, dimensions } = detailQuery.data;
  const isActive = poll.status === 'active';

  const agenda = agendaQuery.data ?? [];
  const activePollIndex = agenda.findIndex((p) => p.id === poll.id);
  const nextPoll = activePollIndex >= 0 ? agenda[activePollIndex + 1] : undefined;
  const isAuthenticated = Boolean(deviceKid);
  const canVote = isActive && isAuthenticated && isVerified;
  const roomName = roomQuery.data?.name;

  return (
    <Stack gap="md" maw={800} mx="auto" mt="xl" px="md">
      <Group>
        <Text component={Link} to="/rooms" size="sm" c="dimmed" style={{ textDecoration: 'none' }}>
          Rooms
        </Text>
        <Text size="sm" c="dimmed">
          /
        </Text>
        {roomName ? (
          <>
            <Text size="sm" c="dimmed">
              {roomName}
            </Text>
            <Text size="sm" c="dimmed">
              /
            </Text>
          </>
        ) : null}
        <Text
          size="sm"
          c="dimmed"
          style={{
            maxWidth: 200,
            overflow: 'hidden',
            textOverflow: 'ellipsis',
            whiteSpace: 'nowrap',
          }}
        >
          {poll.question}
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

      {isActive ? (
        <Group gap="md" mt="xs">
          <PollCountdown secondsLeft={secondsLeft} />
          <AgendaProgress polls={agenda} activePollId={poll.id} />
        </Group>
      ) : null}
      {isActive ? <UpcomingPollPreview poll={nextPoll} /> : null}

      {/* Voting section */}
      {isActive ? (
        <Card
          shadow="sm"
          padding="lg"
          radius="md"
          withBorder
          style={
            hasVoted ? { borderColor: 'var(--mantine-color-green-4)', borderWidth: 2 } : undefined
          }
        >
          <Stack gap="md">
            <Group justify="space-between">
              <Title order={4}>{hasVoted ? 'Your Vote' : 'Cast Your Vote'}</Title>
              {hasVoted ? (
                <Badge color="green" variant="light" leftSection={<IconCheck size={12} />}>
                  Voted
                </Badge>
              ) : null}
            </Group>

            {!hasVoted && canVote ? (
              <Text size="sm" c="dimmed">
                Each slider is a different aspect of the question. Drag to set where you fall
                between the two labeled positions, then submit.{' '}
                <Text component={Link} to="/about#voting" size="sm" c="blue" inherit>
                  How does this work?
                </Text>
              </Text>
            ) : null}

            {!isAuthenticated ? (
              <Alert icon={<IconLock size={16} />} color="yellow">
                <Link to="/signup" style={{ fontWeight: 600 }}>
                  Sign up
                </Link>{' '}
                or{' '}
                <Link to="/login" style={{ fontWeight: 600 }}>
                  log in
                </Link>{' '}
                to vote on this poll.
              </Alert>
            ) : null}

            {isAuthenticated && !isVerified ? (
              <Alert icon={<IconShieldOff size={16} />} color="yellow">
                You need to verify your identity to vote in this room.
                {(() => {
                  const url = buildVerifierUrl(username ?? '');
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

            {isAuthenticated && isVerified && !hasTrustScore ? (
              <Alert icon={<IconShieldOff size={16} />} color="yellow">
                You&apos;re verified, but you need endorsements from trusted members to vote in this
                room. Visit the Trust page to see your status.
              </Alert>
            ) : null}

            {voteMutation.isError ? (
              <Alert icon={<IconAlertTriangle size={16} />} color="red">
                {voteMutation.error.message.includes('trust graph')
                  ? "Your account isn't yet connected to this room's trust network. Ask an existing member to endorse you, or try a different room."
                  : voteMutation.error.message}
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
      ) : (
        <Card shadow="sm" padding="lg" radius="md" withBorder>
          <Alert icon={<IconLock size={16} />} color="gray" title="Poll closed">
            This poll is no longer accepting votes.
          </Alert>
        </Card>
      )}

      {/* Results section */}
      <Card shadow="sm" padding="lg" radius="md" withBorder>
        <Stack gap="md">
          <Group justify="space-between">
            <Title order={4}>Results</Title>
            {resultsQuery.data ? (
              <Text size="sm" c="dimmed">
                {String(resultsQuery.data.voter_count)} vote
                {resultsQuery.data.voter_count !== 1 ? 's' : ''} cast
              </Text>
            ) : null}
          </Group>
          <Text size="xs" c="dimmed">
            Each chart shows how responses are distributed across the spectrum.
          </Text>

          {distributionQuery.isLoading ? <Loader size="sm" /> : null}

          {distributionQuery.isError ? (
            <Alert icon={<IconAlertTriangle size={16} />} color="red">
              Failed to load results: {distributionQuery.error.message}
            </Alert>
          ) : null}

          {resultsQuery.data?.voter_count === 0 ? (
            <Text size="sm" c="dimmed">
              No votes yet. Be the first!
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

function valueToPercent(value: number, min: number, max: number): number {
  return Math.round(((value - min) / (max - min)) * 100);
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
  const pct = valueToPercent(value, dimension.min_value, dimension.max_value);

  return (
    <div>
      <Group justify="space-between" mb="xs">
        <Text size="sm" fw={500}>
          {dimension.name}
        </Text>
        <Text size="sm" c="dimmed">
          {String(pct)}%
        </Text>
      </Group>
      {dimension.description ? (
        <Text size="xs" c="dimmed" mb="xs">
          {dimension.description}
        </Text>
      ) : null}
      <EvidenceCards evidence={dimension.evidence} />
      <div style={{ paddingBottom: 8, paddingLeft: 8, paddingRight: 8 }}>
        <Slider
          value={value}
          onChange={onChange}
          min={dimension.min_value}
          max={dimension.max_value}
          step={0.01}
          disabled={disabled}
          size="lg"
          thumbSize={26}
          label={(val) =>
            `${String(valueToPercent(val, dimension.min_value, dimension.max_value))}%`
          }
          marks={[
            { value: dimension.min_value, label: dimension.min_label ?? 'Low' },
            { value: dimension.max_value, label: dimension.max_label ?? 'High' },
          ]}
          styles={{
            markLabel: {
              fontSize: 'var(--mantine-font-size-xs)',
              whiteSpace: 'normal',
              textAlign: 'center',
              maxWidth: 100,
            },
          }}
        />
      </div>
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
