/**
 * Trust REST API functions — shared across trust and endorsements features.
 *
 * Types and low-level fetch functions live here so both features can import
 * without violating the no-cross-feature-import ESLint rule.
 */

import type { components } from '@/api/generated/rest';
import { signedFetchJson } from '@/api/signing';
import type { CryptoModule } from '@/providers/CryptoProvider';

export type TrustBudget = components['schemas']['BudgetResponse'];
export type Invite = components['schemas']['InviteResponse'];
export type AcceptInviteResult = components['schemas']['AcceptInviteResponse'];
export type CreateInvitePayload = components['schemas']['CreateInviteRequest'];

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
): Promise<components['schemas']['CreateInviteResponse']> {
  return signedFetchJson('/trust/invites', 'POST', deviceKid, privateKey, wasmCrypto, payload);
}

export async function listMyInvites(
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule
): Promise<Invite[]> {
  const response = await signedFetchJson<components['schemas']['InvitesResponse']>(
    '/trust/invites/mine',
    'GET',
    deviceKid,
    privateKey,
    wasmCrypto
  );
  return response.invites;
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
