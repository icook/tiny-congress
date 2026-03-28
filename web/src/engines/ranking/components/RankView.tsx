import { useState } from 'react';
import {
  Badge,
  Button,
  Card,
  Center,
  Group,
  Image,
  Loader,
  SimpleGrid,
  Stack,
  Text,
  Title,
} from '@mantine/core';
import { useCrypto } from '@/providers/CryptoProvider';
import { useDevice } from '@/providers/DeviceProvider';
import { useMatchup, useRecordMatchup, type Round, type Submission } from '../api';

interface Props {
  roomId: string;
  round: Round;
}

function SubmissionCard({
  submission,
  onPick,
  isPicking,
  isSelected,
}: {
  submission: Submission;
  onPick: () => void;
  isPicking: boolean;
  isSelected: boolean;
}) {
  return (
    <Card
      withBorder
      padding="md"
      radius="md"
      onClick={onPick}
      style={{
        cursor: isPicking ? 'not-allowed' : 'pointer',
        outline: isSelected ? '2px solid var(--mantine-color-blue-5)' : undefined,
        transition: 'outline 0.1s ease',
      }}
    >
      <Stack gap="xs">
        {submission.content_type === 'image' && submission.image_key ? (
          <Image
            src={submission.image_key}
            alt="Submission"
            radius="sm"
            fit="contain"
            mah={300}
            fallbackSrc="data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7"
          />
        ) : null}
        {submission.content_type === 'url' && submission.url ? (
          <Card withBorder padding="xs" radius="sm" bg="gray.0">
            <Text size="sm" truncate c="blue" td="underline">
              {submission.url}
            </Text>
          </Card>
        ) : null}
        {submission.caption ? (
          <Text size="sm" c="dimmed">
            {submission.caption}
          </Text>
        ) : null}
        <Button
          onClick={(e) => {
            e.stopPropagation();
            onPick();
          }}
          disabled={isPicking}
          variant={isSelected ? 'filled' : 'light'}
          size="sm"
          fullWidth
        >
          {isSelected ? 'Picking...' : 'Pick this one'}
        </Button>
      </Stack>
    </Card>
  );
}

export function RankView({ roomId, round }: Props) {
  const { deviceKid, privateKey } = useDevice();
  const { crypto } = useCrypto();
  const [matchupCount, setMatchupCount] = useState(0);
  const [selectedId, setSelectedId] = useState<string | null>(null);

  const matchupQuery = useMatchup(roomId, deviceKid, privateKey, crypto);
  const recordMutation = useRecordMatchup(roomId);

  const isAuthenticated = Boolean(deviceKid && privateKey && crypto);

  const handlePick = (winnerId: string, loserId: string) => {
    if (!deviceKid || !privateKey || !crypto) {
      return;
    }
    setSelectedId(winnerId);

    const pair = matchupQuery.data;
    if (!pair) {
      return;
    }

    recordMutation.mutate(
      {
        body: {
          winner_id: winnerId,
          loser_id: loserId,
          submission_a: pair.submission_a.id,
          submission_b: pair.submission_b.id,
        },
        deviceKid,
        privateKey,
        wasmCrypto: crypto,
      },
      {
        onSuccess: () => {
          setMatchupCount((c) => c + 1);
          setSelectedId(null);
        },
        onError: () => {
          setSelectedId(null);
        },
      }
    );
  };

  const handleSkip = () => {
    if (!deviceKid || !privateKey || !crypto) {
      return;
    }

    const pair = matchupQuery.data;
    if (!pair) {
      return;
    }

    recordMutation.mutate(
      {
        body: {
          submission_a: pair.submission_a.id,
          submission_b: pair.submission_b.id,
          skipped: true,
        },
        deviceKid,
        privateKey,
        wasmCrypto: crypto,
      },
      {
        onSuccess: () => {
          setMatchupCount((c) => c + 1);
        },
      }
    );
  };

  if (!isAuthenticated) {
    return (
      <Stack gap="md" mt="md">
        <Text size="sm" c="dimmed">
          Sign in to rank memes.
        </Text>
      </Stack>
    );
  }

  if (matchupQuery.isLoading || recordMutation.isPending) {
    return (
      <Center mt="xl">
        <Loader size="sm" />
      </Center>
    );
  }

  // 404 or no matchups available
  if (matchupQuery.isError || !matchupQuery.data) {
    return (
      <Stack gap="md" mt="md" ta="center">
        <Title order={4}>You've ranked everything!</Title>
        <Text size="sm" c="dimmed">
          No more pairs to rank right now. Check back soon or see the leaderboard.
        </Text>
        {matchupCount > 0 ? (
          <Badge color="green" variant="light" size="lg" mx="auto">
            {matchupCount} {matchupCount === 1 ? 'matchup' : 'matchups'} ranked
          </Badge>
        ) : null}
      </Stack>
    );
  }

  const { submission_a, submission_b } = matchupQuery.data;
  const isPicking = recordMutation.isPending;

  return (
    <Stack gap="md" mt="md">
      <Group justify="space-between" wrap="nowrap">
        <Title order={4}>Which is better?</Title>
        {matchupCount > 0 ? (
          <Badge color="blue" variant="light" size="sm">
            {matchupCount} ranked
          </Badge>
        ) : null}
      </Group>

      <Text size="sm" c="dimmed">
        Round {String(round.round_number)} — tap your pick
      </Text>

      <SimpleGrid cols={{ base: 1, sm: 2 }} spacing="md">
        <SubmissionCard
          submission={submission_a}
          onPick={() => {
            handlePick(submission_a.id, submission_b.id);
          }}
          isPicking={isPicking}
          isSelected={selectedId === submission_a.id}
        />
        <SubmissionCard
          submission={submission_b}
          onPick={() => {
            handlePick(submission_b.id, submission_a.id);
          }}
          isPicking={isPicking}
          isSelected={selectedId === submission_b.id}
        />
      </SimpleGrid>

      {recordMutation.error ? (
        <Text size="sm" c="red">
          {recordMutation.error.message}
        </Text>
      ) : null}

      <Button variant="subtle" color="gray" onClick={handleSkip} disabled={isPicking} size="sm">
        Can't decide — skip
      </Button>
    </Stack>
  );
}
