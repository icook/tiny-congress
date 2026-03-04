/**
 * Verification API client
 * Queries endorsement status for the authenticated user
 */

import { signedFetchJson } from '@/api/signing';
import type { CryptoModule } from '@/providers/CryptoProvider';

export interface Endorsement {
  id: string;
  subject_id: string;
  topic: string;
  issuer_id: string | null;
  created_at: string;
  revoked: boolean;
}

export interface EndorsementsListResponse {
  endorsements: Endorsement[];
}

export async function getMyEndorsements(
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule
): Promise<EndorsementsListResponse> {
  return signedFetchJson('/me/endorsements', 'GET', deviceKid, privateKey, wasmCrypto);
}
