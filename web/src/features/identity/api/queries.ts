/**
 * TanStack Query hooks for identity API
 */

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import type { CryptoModule } from '@/providers/CryptoProvider';
import {
  listDevices,
  renameDevice,
  revokeDevice,
  signup,
  type DeviceListResponse,
  type SignupRequest,
  type SignupResponse,
} from './client';

/**
 * Mutation hook for user signup
 */
export function useSignup() {
  return useMutation<SignupResponse, Error, SignupRequest>({
    mutationFn: signup,
  });
}

/**
 * Query hook for listing devices
 */
export function useListDevices(
  deviceKid: string | null,
  privateKey: Uint8Array | null,
  wasmCrypto: CryptoModule | null
) {
  return useQuery<DeviceListResponse>({
    queryKey: ['devices', deviceKid],
    queryFn: () => {
      if (!deviceKid || !privateKey || !wasmCrypto) {
        throw new Error('Not authenticated');
      }
      return listDevices(deviceKid, privateKey, wasmCrypto);
    },
    enabled: Boolean(deviceKid && privateKey && wasmCrypto),
  });
}

/**
 * Mutation hook for revoking a device
 */
export function useRevokeDevice(
  deviceKid: string | null,
  privateKey: Uint8Array | null,
  wasmCrypto: CryptoModule | null
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (targetKid: string) => {
      if (!deviceKid || !privateKey || !wasmCrypto) {
        throw new Error('Not authenticated');
      }
      await revokeDevice(targetKid, deviceKid, privateKey, wasmCrypto);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['devices'] });
    },
  });
}

/**
 * Mutation hook for renaming a device
 */
export function useRenameDevice(
  deviceKid: string | null,
  privateKey: Uint8Array | null,
  wasmCrypto: CryptoModule | null
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async ({ targetKid, name }: { targetKid: string; name: string }) => {
      if (!deviceKid || !privateKey || !wasmCrypto) {
        throw new Error('Not authenticated');
      }
      await renameDevice(targetKid, name, deviceKid, privateKey, wasmCrypto);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['devices'] });
    },
  });
}
