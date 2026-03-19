/**
 * Trust API client
 * Type-safe REST client for trust scores, budget, endorsements, and invites.
 *
 * Shared API functions (budget, invites, endorsements) live in @/api/trust and
 * @/api/endorsements so the endorsements feature can also import them without
 * violating the no-cross-feature-import ESLint rule.
 */

import { signedFetchJson } from '@/api/signing';
import type { CryptoModule } from '@/providers/CryptoProvider';

// Re-export shared types and functions so feature consumers can import from
// the trust feature barrel without knowing the internal split.
export type { Endorsement, EndorsementsListResponse } from '@/api/endorsements';
export { getMyEndorsements } from '@/api/endorsements';
export type { TrustBudget, Invite, AcceptInviteResult, CreateInvitePayload } from '@/api/trust';
export {
  getMyBudget,
  createInvite,
  listMyInvites,
  acceptInvite,
  revokeEndorsement,
} from '@/api/trust';

// === Trust-feature-only types ===

export interface Denouncement {
  id: string;
  target_id: string;
  target_username: string;
  reason: string;
  created_at: string;
}

export interface DenouncementPayload {
  target_id: string;
  reason: string;
}

export interface AccountLookup {
  id: string;
  username: string;
}

export interface ScoreSnapshot {
  subject_id: string;
  distance: number;
  path_diversity: number;
  computed_at: string;
}

export interface EndorsePayload {
  subject_id: string;
  weight: number;
  attestation: Record<string, unknown>;
}

// === Trust-feature-only API functions ===

export async function getMyScores(
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule
): Promise<ScoreSnapshot[]> {
  return signedFetchJson('/trust/scores/me', 'GET', deviceKid, privateKey, wasmCrypto);
}

export async function endorse(
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule,
  payload: EndorsePayload
): Promise<void> {
  await signedFetchJson('/trust/endorse', 'POST', deviceKid, privateKey, wasmCrypto, payload);
}

export async function denounce(
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule,
  payload: DenouncementPayload
): Promise<{ message: string }> {
  return signedFetchJson('/trust/denounce', 'POST', deviceKid, privateKey, wasmCrypto, payload);
}

export async function listMyDenouncements(
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule
): Promise<Denouncement[]> {
  return signedFetchJson('/trust/denouncements/mine', 'GET', deviceKid, privateKey, wasmCrypto);
}

export async function lookupAccount(
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule,
  username: string
): Promise<AccountLookup> {
  return signedFetchJson(
    `/accounts/lookup?username=${encodeURIComponent(username)}`,
    'GET',
    deviceKid,
    privateKey,
    wasmCrypto
  );
}
