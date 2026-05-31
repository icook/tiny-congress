/**
 * RankingEngineView — room detail page for the ranking (meme tournament) engine.
 *
 * Shows tabs for submitting memes, ranking pairs, viewing the leaderboard,
 * and browsing the Hall of Fame.
 */

import { Center, Loader, Stack, Tabs, Text } from '@mantine/core';
import type { EngineViewProps } from '../api';
import { useCurrentRounds, useListRounds } from './api';
import { HallOfFame } from './components/HallOfFame';
import { Leaderboard } from './components/Leaderboard';
import { RankView } from './components/RankView';
import { SubmitView } from './components/SubmitView';

function resolveDefaultTab(submittingRound: boolean, rankingRound: boolean): string {
  if (rankingRound) {
    return 'rank';
  }
  if (submittingRound) {
    return 'submit';
  }
  return 'leaderboard';
}

export function RankingEngineView({ roomId }: EngineViewProps) {
  const { data: currentRounds, isLoading: currentLoading } = useCurrentRounds(roomId);
  const { data: allRounds } = useListRounds(roomId);

  const submittingRound = currentRounds?.find((r) => r.status === 'submitting');
  const rankingRound = currentRounds?.find((r) => r.status === 'ranking');

  // Most recent closed round for leaderboard default
  const mostRecentRound =
    rankingRound ??
    submittingRound ??
    allRounds?.slice().sort((a, b) => b.round_number - a.round_number)[0];

  const defaultTab = resolveDefaultTab(Boolean(submittingRound), Boolean(rankingRound));

  if (currentLoading) {
    return (
      <Center mt="xl">
        <Loader size="sm" />
      </Center>
    );
  }

  return (
    <Stack gap="lg" maw={900} mx="auto" mt="xl" px="md">
      <Tabs defaultValue={defaultTab}>
        <Tabs.List>
          <Tabs.Tab value="submit" disabled={!submittingRound}>
            Submit
          </Tabs.Tab>
          <Tabs.Tab value="rank" disabled={!rankingRound}>
            Rank
          </Tabs.Tab>
          <Tabs.Tab value="leaderboard">Leaderboard</Tabs.Tab>
          <Tabs.Tab value="hall-of-fame">Hall of Fame</Tabs.Tab>
        </Tabs.List>

        <Tabs.Panel value="submit">
          {submittingRound ? (
            <SubmitView roomId={roomId} round={submittingRound} />
          ) : (
            <Text size="sm" c="dimmed" mt="md">
              No submission window is open right now.
            </Text>
          )}
        </Tabs.Panel>

        <Tabs.Panel value="rank">
          {rankingRound ? (
            <RankView roomId={roomId} round={rankingRound} />
          ) : (
            <Text size="sm" c="dimmed" mt="md">
              No ranking window is open right now.
            </Text>
          )}
        </Tabs.Panel>

        <Tabs.Panel value="leaderboard">
          {mostRecentRound ? (
            <Leaderboard
              roomId={roomId}
              roundId={mostRecentRound.id}
              roundStatus={mostRecentRound.status}
            />
          ) : (
            <Text size="sm" c="dimmed" mt="md">
              No rounds have started yet.
            </Text>
          )}
        </Tabs.Panel>

        <Tabs.Panel value="hall-of-fame">
          <HallOfFame roomId={roomId} />
        </Tabs.Panel>
      </Tabs>
    </Stack>
  );
}
