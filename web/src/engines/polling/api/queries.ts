/**
 * TanStack Query hooks for rooms API
 */

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import type { CryptoModule } from '@/providers/CryptoProvider';
import {
  castVote,
  createSuggestion,
  fetchMyCapabilities,
  getAgenda,
  getMyVotes,
  getPollDetail,
  getPollDistribution,
  getPollResults,
  getPollTraces,
  getRoom,
  listPolls,
  listRooms,
  listSuggestions,
  type BotTrace,
  type DimensionVote,
  type MyCapabilitiesResponse,
  type Poll,
  type PollDetail,
  type PollDistribution,
  type PollResults,
  type Room,
  type Suggestion,
  type Vote,
} from './client';

export function useRooms() {
  return useQuery<Room[]>({
    queryKey: ['rooms'],
    queryFn: listRooms,
  });
}

export function useRoom(roomId: string) {
  return useQuery<Room>({
    queryKey: ['room', roomId],
    queryFn: () => getRoom(roomId),
    enabled: Boolean(roomId),
  });
}

export function usePolls(roomId: string) {
  return useQuery<Poll[]>({
    queryKey: ['polls', roomId],
    queryFn: () => listPolls(roomId),
    enabled: Boolean(roomId),
  });
}

export function usePollDetail(roomId: string, pollId: string) {
  return useQuery<PollDetail>({
    queryKey: ['poll-detail', roomId, pollId],
    queryFn: () => getPollDetail(roomId, pollId),
    enabled: Boolean(roomId && pollId),
    refetchInterval: 20_000,
  });
}

export function useAgenda(roomId: string) {
  return useQuery<Poll[]>({
    queryKey: ['agenda', roomId],
    queryFn: () => getAgenda(roomId),
    enabled: Boolean(roomId),
    refetchInterval: 20_000,
  });
}

export function usePollResults(roomId: string, pollId: string) {
  return useQuery<PollResults>({
    queryKey: ['poll-results', roomId, pollId],
    queryFn: () => getPollResults(roomId, pollId),
    enabled: Boolean(roomId && pollId),
    refetchInterval: 20_000,
  });
}

export function usePollTraces(roomId: string, pollId: string) {
  return useQuery<BotTrace[]>({
    queryKey: ['poll-traces', roomId, pollId],
    queryFn: () => getPollTraces(roomId, pollId),
    enabled: Boolean(roomId && pollId),
  });
}

export function usePollDistribution(roomId: string, pollId: string) {
  return useQuery<PollDistribution>({
    queryKey: ['poll-distribution', roomId, pollId],
    queryFn: () => getPollDistribution(roomId, pollId),
    enabled: Boolean(roomId && pollId),
    refetchInterval: 20_000,
  });
}

export function useMyVotes(
  roomId: string,
  pollId: string,
  deviceKid: string | null,
  privateKey: CryptoKey | null,
  wasmCrypto: CryptoModule | null
) {
  return useQuery<Vote[]>({
    queryKey: ['my-votes', pollId, deviceKid],
    queryFn: () => {
      if (!deviceKid || !privateKey || !wasmCrypto) {
        throw new Error('Not authenticated');
      }
      return getMyVotes(roomId, pollId, deviceKid, privateKey, wasmCrypto);
    },
    enabled: Boolean(roomId && pollId && deviceKid && privateKey && wasmCrypto),
  });
}

export function useMyCapabilities(
  roomId: string | undefined,
  deviceKid: string | null,
  privateKey: CryptoKey | null,
  wasmCrypto: CryptoModule | null
) {
  return useQuery<MyCapabilitiesResponse>({
    queryKey: ['rooms', roomId, 'my-capabilities'],
    queryFn: () => {
      if (!roomId || !deviceKid || !privateKey || !wasmCrypto) {
        throw new Error('Not authenticated');
      }
      return fetchMyCapabilities(roomId, deviceKid, privateKey, wasmCrypto);
    },
    enabled: Boolean(roomId && deviceKid && privateKey && wasmCrypto),
  });
}

export function useCastVote(
  roomId: string,
  pollId: string,
  deviceKid: string | null,
  privateKey: CryptoKey | null,
  wasmCrypto: CryptoModule | null
) {
  const queryClient = useQueryClient();

  return useMutation<Vote[], Error, DimensionVote[]>({
    mutationFn: async (votes: DimensionVote[]) => {
      if (!deviceKid || !privateKey || !wasmCrypto) {
        throw new Error('Not authenticated');
      }
      return castVote(roomId, pollId, votes, deviceKid, privateKey, wasmCrypto);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['my-votes', pollId] });
      void queryClient.invalidateQueries({ queryKey: ['poll-results', roomId, pollId] });
      void queryClient.invalidateQueries({ queryKey: ['poll-distribution', roomId, pollId] });
    },
  });
}

export function useSuggestions(roomId: string, pollId: string) {
  return useQuery<Suggestion[]>({
    queryKey: ['suggestions', roomId, pollId],
    queryFn: () => listSuggestions(roomId, pollId),
    enabled: Boolean(roomId && pollId),
    refetchInterval: 15_000,
  });
}

export function useCreateSuggestion(
  roomId: string,
  pollId: string,
  deviceKid: string | null,
  privateKey: CryptoKey | null,
  wasmCrypto: CryptoModule | null
) {
  const queryClient = useQueryClient();

  return useMutation<Suggestion, Error, string>({
    mutationFn: async (suggestionText: string) => {
      if (!deviceKid || !privateKey || !wasmCrypto) {
        throw new Error('Not authenticated');
      }
      return createSuggestion(roomId, pollId, suggestionText, deviceKid, privateKey, wasmCrypto);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['suggestions', roomId, pollId] });
    },
  });
}
