/**
 * Verification query hooks
 */

import { useQuery } from '@tanstack/react-query';
import type { CryptoModule } from '@/providers/CryptoProvider';
import { getMyEndorsements } from './client';

export interface VerificationStatus {
  isVerified: boolean;
  method?: string;
  verifiedAt?: string;
}

export function useVerificationStatus(
  deviceKid: string | null,
  privateKey: CryptoKey | null,
  wasmCrypto: CryptoModule | null
) {
  return useQuery({
    queryKey: ['verification-status', deviceKid],
    queryFn: async (): Promise<VerificationStatus> => {
      if (!deviceKid || !privateKey || !wasmCrypto) {
        throw new Error('Not authenticated');
      }

      const response = await getMyEndorsements(deviceKid, privateKey, wasmCrypto);
      const verified = response.endorsements.find(
        (e) => e.topic === 'identity_verified' && !e.revoked
      );

      if (!verified) {
        return { isVerified: false };
      }

      return {
        isVerified: true,
        verifiedAt: verified.created_at,
      };
    },
    enabled: Boolean(deviceKid && privateKey && wasmCrypto),
    staleTime: 30_000,
  });
}
