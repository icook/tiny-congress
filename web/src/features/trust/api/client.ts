/**
 * Trust API client
 * Type-safe REST client for trust scores, budget, endorsements, and invites
 */

import { signedFetchJson } from '@/api/signing';
import type { CryptoModule } from '@/providers/CryptoProvider';

// === Types ===

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

export interface TrustBudget {
  slots_total: number;
  slots_used: number;
  slots_available: number;
  denouncements_total: number;
  denouncements_used: number;
  denouncements_available: number;
}

export interface Invite {
  id: string;
  delivery_method: string;
  accepted_by: string | null;
  expires_at: string;
  accepted_at: string | null;
}

export interface AcceptInviteResult {
  endorser_id: string;
  accepted_at: string;
}

export interface EndorsePayload {
  subject_id: string;
  weight: number;
  attestation: Record<string, unknown>;
}

export interface RevokePayload {
  subject_id: string;
}

export interface CreateInvitePayload {
  envelope: string;
  delivery_method: string;
  relationship_depth?: string;
  attestation: Record<string, unknown>;
}

// === API functions ===

export async function getMyScores(
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule
): Promise<ScoreSnapshot[]> {
  return signedFetchJson('/trust/scores/me', 'GET', deviceKid, privateKey, wasmCrypto);
}

export async function getMyBudget(
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule
): Promise<TrustBudget> {
  return signedFetchJson('/trust/budget', 'GET', deviceKid, privateKey, wasmCrypto);
}

export async function createInvite(
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule,
  payload: CreateInvitePayload
): Promise<Invite> {
  return signedFetchJson('/trust/invites', 'POST', deviceKid, privateKey, wasmCrypto, payload);
}

export async function listMyInvites(
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule
): Promise<Invite[]> {
  return signedFetchJson('/trust/invites/mine', 'GET', deviceKid, privateKey, wasmCrypto);
}

export async function acceptInvite(
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule,
  inviteId: string
): Promise<AcceptInviteResult> {
  return signedFetchJson(
    `/trust/invites/${inviteId}/accept`,
    'POST',
    deviceKid,
    privateKey,
    wasmCrypto
  );
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

export async function revokeEndorsement(
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule,
  subjectId: string
): Promise<void> {
  const payload: RevokePayload = { subject_id: subjectId };
  await signedFetchJson('/trust/revoke', 'POST', deviceKid, privateKey, wasmCrypto, payload);
}
