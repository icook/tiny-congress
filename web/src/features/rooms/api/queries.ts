/**
 * TanStack Query hooks for rooms API
 */

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import type { CryptoModule } from '@/providers/CryptoProvider';
import {
  castVote,
  getMyVotes,
  getPollDetail,
  getPollResults,
  listPolls,
  listRooms,
  type DimensionVote,
  type Poll,
  type PollDetail,
  type PollResults,
  type Room,
  type Vote,
} from './client';

export function useRooms() {
  return useQuery<Room[]>({
    queryKey: ['rooms'],
    queryFn: listRooms,
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
    queryKey: ['poll-detail', pollId],
    queryFn: () => getPollDetail(roomId, pollId),
    enabled: Boolean(roomId && pollId),
  });
}

export function usePollResults(roomId: string, pollId: string) {
  return useQuery<PollResults>({
    queryKey: ['poll-results', pollId],
    queryFn: () => getPollResults(roomId, pollId),
    enabled: Boolean(roomId && pollId),
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
      void queryClient.invalidateQueries({ queryKey: ['poll-results', pollId] });
    },
  });
}
