/**
 * TanStack Query hooks for the ranking engine
 */

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import type { CryptoModule } from '@/providers/CryptoProvider';
import {
  getCurrentRounds,
  getHallOfFame,
  getLeaderboard,
  getMatchup,
  listRounds,
  recordMatchup,
  submitMeme,
  type HallOfFameEntry,
  type LeaderboardResponse,
  type MatchupPair,
  type MatchupResult,
  type RecordMatchupBody,
  type Round,
  type Submission,
  type SubmitBody,
} from './client';

export function useCurrentRounds(roomId: string) {
  return useQuery<Round[]>({
    queryKey: ['ranking', 'rounds', 'current', roomId],
    queryFn: () => getCurrentRounds(roomId),
    enabled: Boolean(roomId),
    refetchInterval: 15_000,
  });
}

export function useListRounds(roomId: string) {
  return useQuery<Round[]>({
    queryKey: ['ranking', 'rounds', roomId],
    queryFn: () => listRounds(roomId),
    enabled: Boolean(roomId),
    refetchInterval: 15_000,
  });
}

export function useLeaderboard(roomId: string, roundId: string | undefined) {
  return useQuery<LeaderboardResponse>({
    queryKey: ['ranking', 'leaderboard', roomId, roundId],
    queryFn: () => {
      if (!roundId) {
        throw new Error('roundId is required');
      }
      return getLeaderboard(roomId, roundId);
    },
    enabled: Boolean(roomId && roundId),
    refetchInterval: 15_000,
  });
}

export function useHallOfFame(roomId: string) {
  return useQuery<HallOfFameEntry[]>({
    queryKey: ['ranking', 'hallOfFame', roomId],
    queryFn: () => getHallOfFame(roomId),
    enabled: Boolean(roomId),
  });
}

export function useMatchup(
  roomId: string,
  deviceKid: string | null,
  privateKey: CryptoKey | null,
  wasmCrypto: CryptoModule | null
) {
  return useQuery<MatchupPair>({
    queryKey: ['ranking', 'matchup', roomId],
    queryFn: () => {
      if (!deviceKid || !privateKey || !wasmCrypto) {
        throw new Error('Not authenticated');
      }
      return getMatchup(roomId, deviceKid, privateKey, wasmCrypto);
    },
    enabled: Boolean(roomId && deviceKid && privateKey && wasmCrypto),
    retry: false,
  });
}

export function useSubmitMeme(roomId: string) {
  const queryClient = useQueryClient();

  return useMutation<
    Submission,
    Error,
    { body: SubmitBody; deviceKid: string; privateKey: CryptoKey; wasmCrypto: CryptoModule }
  >({
    mutationFn: ({ body, deviceKid, privateKey, wasmCrypto }) =>
      submitMeme(roomId, body, deviceKid, privateKey, wasmCrypto),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['ranking', 'rounds', 'current', roomId] });
    },
  });
}

export function useRecordMatchup(roomId: string) {
  const queryClient = useQueryClient();

  return useMutation<
    MatchupResult,
    Error,
    { body: RecordMatchupBody; deviceKid: string; privateKey: CryptoKey; wasmCrypto: CryptoModule }
  >({
    mutationFn: ({ body, deviceKid, privateKey, wasmCrypto }) =>
      recordMatchup(roomId, body, deviceKid, privateKey, wasmCrypto),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['ranking', 'matchup', roomId] });
      void queryClient.invalidateQueries({ queryKey: ['ranking', 'leaderboard', roomId] });
    },
  });
}
