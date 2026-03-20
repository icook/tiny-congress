/**
 * TanStack Query hooks for trust API.
 *
 * Shared hooks (useTrustBudget, useMyEndorsementsList, useMyInvites,
 * useCreateInvite, useAcceptInvite, useRevokeEndorsement) are re-exported
 * from @/api/trustQueries so the endorsements feature can import them too
 * without violating the no-cross-feature-import ESLint boundary rule.
 */

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import type { CryptoModule } from '@/providers/CryptoProvider';
import {
  denounce,
  endorse,
  getMyScores,
  listMyDenouncements,
  lookupAccount,
  type DenouncementPayload,
  type EndorsePayload,
} from './client';

// Re-export shared hooks so feature consumers only need to import from trust.
export {
  useTrustBudget,
  useMyEndorsementsList,
  useMyInvites,
  useCreateInvite,
  useAcceptInvite,
  useRevokeEndorsement,
} from '@/api/trustQueries';

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
