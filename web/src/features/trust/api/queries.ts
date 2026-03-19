/**
 * TanStack Query hooks for trust API
 */

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import type { CryptoModule } from '@/providers/CryptoProvider';
import {
  acceptInvite,
  createInvite,
  denounce,
  endorse,
  getMyBudget,
  getMyEndorsements,
  getMyScores,
  listMyDenouncements,
  listMyInvites,
  lookupAccount,
  revokeEndorsement,
  type CreateInvitePayload,
  type DenouncementPayload,
  type EndorsePayload,
} from './client';

export function useTrustScores(
  deviceKid: string | null,
  privateKey: CryptoKey | null,
  wasmCrypto: CryptoModule | null
) {
  return useQuery({
    queryKey: ['trust-scores', deviceKid],
    queryFn: async () => {
      if (!deviceKid || !privateKey || !wasmCrypto) {
        throw new Error('Not authenticated');
      }
      return getMyScores(deviceKid, privateKey, wasmCrypto);
    },
    enabled: Boolean(deviceKid && privateKey && wasmCrypto),
    staleTime: 30_000,
  });
}

export function useTrustBudget(
  deviceKid: string | null,
  privateKey: CryptoKey | null,
  wasmCrypto: CryptoModule | null
) {
  return useQuery({
    queryKey: ['trust-budget', deviceKid],
    queryFn: async () => {
      if (!deviceKid || !privateKey || !wasmCrypto) {
        throw new Error('Not authenticated');
      }
      return getMyBudget(deviceKid, privateKey, wasmCrypto);
    },
    enabled: Boolean(deviceKid && privateKey && wasmCrypto),
    staleTime: 30_000,
  });
}

export function useMyEndorsementsList(
  deviceKid: string | null,
  privateKey: CryptoKey | null,
  wasmCrypto: CryptoModule | null
) {
  return useQuery({
    queryKey: ['trust-endorsements', deviceKid],
    queryFn: async () => {
      if (!deviceKid || !privateKey || !wasmCrypto) {
        throw new Error('Not authenticated');
      }
      return getMyEndorsements(deviceKid, privateKey, wasmCrypto);
    },
    enabled: Boolean(deviceKid && privateKey && wasmCrypto),
    staleTime: 30_000,
  });
}

export function useMyInvites(
  deviceKid: string | null,
  privateKey: CryptoKey | null,
  wasmCrypto: CryptoModule | null
) {
  return useQuery({
    queryKey: ['trust-invites', deviceKid],
    queryFn: async () => {
      if (!deviceKid || !privateKey || !wasmCrypto) {
        throw new Error('Not authenticated');
      }
      return listMyInvites(deviceKid, privateKey, wasmCrypto);
    },
    enabled: Boolean(deviceKid && privateKey && wasmCrypto),
    staleTime: 30_000,
  });
}

export function useCreateInvite(
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule
) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (payload: CreateInvitePayload) =>
      createInvite(deviceKid, privateKey, wasmCrypto, payload),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['trust-invites'] });
    },
  });
}

export function useAcceptInvite(
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule
) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (inviteId: string) => acceptInvite(deviceKid, privateKey, wasmCrypto, inviteId),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['trust-scores'] });
      void queryClient.invalidateQueries({ queryKey: ['trust-invites'] });
      void queryClient.invalidateQueries({ queryKey: ['trust-budget'] });
      void queryClient.invalidateQueries({ queryKey: ['trust-endorsements'] });
      void queryClient.invalidateQueries({ queryKey: ['verification-status'] });
    },
  });
}

export function useEndorse(deviceKid: string, privateKey: CryptoKey, wasmCrypto: CryptoModule) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (payload: EndorsePayload) => endorse(deviceKid, privateKey, wasmCrypto, payload),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['trust-budget'] });
      void queryClient.invalidateQueries({ queryKey: ['trust-scores'] });
    },
  });
}

export function useMyDenouncements(
  deviceKid: string | null,
  privateKey: CryptoKey | null,
  wasmCrypto: CryptoModule | null
) {
  return useQuery({
    queryKey: ['trust-denouncements', deviceKid],
    queryFn: async () => {
      if (!deviceKid || !privateKey || !wasmCrypto) {
        throw new Error('Not authenticated');
      }
      return listMyDenouncements(deviceKid, privateKey, wasmCrypto);
    },
    enabled: Boolean(deviceKid && privateKey && wasmCrypto),
    staleTime: 30_000,
  });
}

export function useDenounce(deviceKid: string, privateKey: CryptoKey, wasmCrypto: CryptoModule) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (payload: DenouncementPayload) =>
      denounce(deviceKid, privateKey, wasmCrypto, payload),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['trust-denouncements'] });
      void queryClient.invalidateQueries({ queryKey: ['trust-budget'] });
      void queryClient.invalidateQueries({ queryKey: ['trust-scores'] });
    },
  });
}

export function useLookupAccount(
  deviceKid: string | null,
  privateKey: CryptoKey | null,
  wasmCrypto: CryptoModule | null,
  username: string
) {
  return useQuery({
    queryKey: ['account-lookup', username],
    queryFn: async () => {
      if (!deviceKid || !privateKey || !wasmCrypto) {
        throw new Error('Not authenticated');
      }
      return lookupAccount(deviceKid, privateKey, wasmCrypto, username);
    },
    enabled: Boolean(deviceKid && privateKey && wasmCrypto && username.trim()),
    staleTime: 60_000,
    retry: false,
  });
}

export function useRevokeEndorsement(
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule
) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (subjectId: string) =>
      revokeEndorsement(deviceKid, privateKey, wasmCrypto, subjectId),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['trust-budget'] });
      void queryClient.invalidateQueries({ queryKey: ['trust-scores'] });
      void queryClient.invalidateQueries({ queryKey: ['trust-endorsements'] });
    },
  });
}
