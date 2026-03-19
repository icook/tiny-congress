import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import type { CryptoModule } from '@/providers/CryptoProvider';
import type { CreateInvitePayload } from '../types';
import {
  acceptInvite,
  createInvite,
  getMyEndorsements,
  getMyInvites,
  getTrustBudget,
  revokeEndorsement,
} from './client';

export function useTrustBudget(
  deviceKid: string | null,
  privateKey: CryptoKey | null,
  crypto: CryptoModule | undefined
) {
  return useQuery({
    queryKey: ['trust-budget', deviceKid],
    queryFn: async () => {
      if (!deviceKid || !privateKey || !crypto) {
        throw new Error('Not authenticated');
      }
      return getTrustBudget(deviceKid, privateKey, crypto);
    },
    enabled: Boolean(deviceKid && privateKey && crypto),
    staleTime: 30_000,
  });
}

export function useMyEndorsementsList(
  deviceKid: string | null,
  privateKey: CryptoKey | null,
  crypto: CryptoModule | undefined
) {
  return useQuery({
    queryKey: ['my-endorsements', deviceKid],
    queryFn: async () => {
      if (!deviceKid || !privateKey || !crypto) {
        throw new Error('Not authenticated');
      }
      return getMyEndorsements(deviceKid, privateKey, crypto);
    },
    enabled: Boolean(deviceKid && privateKey && crypto),
    staleTime: 30_000,
  });
}

export function useMyInvites(
  deviceKid: string | null,
  privateKey: CryptoKey | null,
  crypto: CryptoModule | undefined
) {
  return useQuery({
    queryKey: ['trust-invites', deviceKid],
    queryFn: async () => {
      if (!deviceKid || !privateKey || !crypto) {
        throw new Error('Not authenticated');
      }
      return getMyInvites(deviceKid, privateKey, crypto);
    },
    enabled: Boolean(deviceKid && privateKey && crypto),
    staleTime: 30_000,
  });
}

export function useCreateInvite(deviceKid: string, privateKey: CryptoKey, crypto: CryptoModule) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (payload: CreateInvitePayload) =>
      createInvite(deviceKid, privateKey, crypto, payload),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['trust-invites'] });
    },
  });
}

export function useAcceptInvite(deviceKid: string, privateKey: CryptoKey, crypto: CryptoModule) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (inviteId: string) => acceptInvite(deviceKid, privateKey, crypto, inviteId),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['my-endorsements'] });
      void queryClient.invalidateQueries({ queryKey: ['trust-invites'] });
      void queryClient.invalidateQueries({ queryKey: ['trust-budget'] });
      void queryClient.invalidateQueries({ queryKey: ['verification-status'] });
    },
  });
}

export function useRevokeEndorsement(
  deviceKid: string,
  privateKey: CryptoKey,
  crypto: CryptoModule
) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (subjectId: string) => revokeEndorsement(deviceKid, privateKey, crypto, subjectId),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['my-endorsements'] });
      void queryClient.invalidateQueries({ queryKey: ['trust-budget'] });
    },
  });
}
