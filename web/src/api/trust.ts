/**
 * Trust REST API functions — shared across trust and endorsements features.
 *
 * Types and low-level fetch functions live here so both features can import
 * without violating the no-cross-feature-import ESLint rule.
 */

import { signedFetchJson } from '@/api/signing';
import type { CryptoModule } from '@/providers/CryptoProvider';

export interface TrustBudget {
  slots_total: number;
  slots_used: number;
  slots_available: number;
  out_of_slot_count: number;
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

export interface CreateInvitePayload {
  envelope: string;
  delivery_method: string;
  relationship_depth?: string;
  attestation: Record<string, unknown>;
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

export async function revokeEndorsement(
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule,
  subjectId: string
): Promise<void> {
  await signedFetchJson('/trust/revoke', 'POST', deviceKid, privateKey, wasmCrypto, {
    subject_id: subjectId,
  });
}
