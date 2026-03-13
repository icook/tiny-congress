import { signedFetchJson } from '@/api/signing';
import type { CryptoModule } from '@/providers/CryptoProvider';
import type {
  AcceptInviteResponse,
  BudgetResponse,
  CreateInvitePayload,
  CreateInviteResponse,
  EndorsementsListResponse,
  InvitesListResponse,
} from '../types';

export async function getMyEndorsements(
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule
): Promise<EndorsementsListResponse> {
  return signedFetchJson('/me/endorsements', 'GET', deviceKid, privateKey, wasmCrypto);
}

export async function getTrustBudget(
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule
): Promise<BudgetResponse> {
  return signedFetchJson('/trust/budget', 'GET', deviceKid, privateKey, wasmCrypto);
}

export async function getMyInvites(
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule
): Promise<InvitesListResponse> {
  return signedFetchJson('/trust/invites/mine', 'GET', deviceKid, privateKey, wasmCrypto);
}

export async function createInvite(
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule,
  payload: CreateInvitePayload
): Promise<CreateInviteResponse> {
  return signedFetchJson('/trust/invites', 'POST', deviceKid, privateKey, wasmCrypto, payload);
}

export async function acceptInvite(
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule,
  inviteId: string
): Promise<AcceptInviteResponse> {
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
