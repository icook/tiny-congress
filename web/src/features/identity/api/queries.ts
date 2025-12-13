/**
 * TanStack Query option factories for identity operations
 * Centralized query definitions for consistent caching and refetching
 */

import { useMutation, useQueryClient, queryOptions } from '@tanstack/react-query';
import {
  addDevice,
  approveRecovery,
  createEndorsement,
  getRecoveryPolicy,
  issueChallenge,
  revokeDevice,
  revokeEndorsement,
  rotateRoot,
  setRecoveryPolicy,
  signup,
  verifyChallenge,
  type AddDeviceRequest,
  type AddDeviceResponse,
  type ChallengeRequest,
  type ChallengeResponse,
  type EndorsementCreateRequest,
  type EndorsementCreateResponse,
  type EndorsementRevokeRequest,
  type RecoveryApprovalRequest,
  type RecoveryApprovalResponse,
  type RecoveryPolicyRequest,
  type RecoveryPolicyResponse,
  type RevokeDeviceRequest,
  type RootRotationRequest,
  type RootRotationResponse,
  type SignupRequest,
  type SignupResponse,
  type VerifyRequest,
  type VerifyResponse,
} from './client';

// === Query Keys ===
export const identityKeys = {
  all: ['identity'] as const,
  recoveryPolicy: (accountId: string) => [...identityKeys.all, 'recovery', accountId] as const,
};

// === Query Factories ===

export const recoveryPolicyQuery = (accountId: string) =>
  queryOptions({
    queryKey: identityKeys.recoveryPolicy(accountId),
    queryFn: () => getRecoveryPolicy(accountId),
    staleTime: 5 * 60 * 1000, // 5 minutes
  });

// === Mutation Hooks ===

// Auth mutations
export function useSignup() {
  const queryClient = useQueryClient();
  return useMutation<SignupResponse, Error, SignupRequest>({
    mutationFn: signup,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: identityKeys.all });
    },
  });
}

export function useIssueChallenge() {
  return useMutation<ChallengeResponse, Error, ChallengeRequest>({
    mutationFn: issueChallenge,
  });
}

export function useVerifyChallenge() {
  const queryClient = useQueryClient();
  return useMutation<VerifyResponse, Error, VerifyRequest>({
    mutationFn: verifyChallenge,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: identityKeys.all });
    },
  });
}

// Device mutations
export function useAddDevice() {
  const queryClient = useQueryClient();
  return useMutation<AddDeviceResponse, Error, AddDeviceRequest>({
    mutationFn: addDevice,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: identityKeys.all });
    },
  });
}

export function useRevokeDevice() {
  const queryClient = useQueryClient();
  return useMutation<void, Error, { deviceId: string; request: RevokeDeviceRequest }>({
    mutationFn: ({ deviceId, request }) => revokeDevice(deviceId, request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: identityKeys.all });
    },
  });
}

// Endorsement mutations
export function useCreateEndorsement() {
  const queryClient = useQueryClient();
  return useMutation<EndorsementCreateResponse, Error, EndorsementCreateRequest>({
    mutationFn: createEndorsement,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: identityKeys.all });
    },
  });
}

export function useRevokeEndorsement() {
  const queryClient = useQueryClient();
  return useMutation<void, Error, { endorsementId: string; request: EndorsementRevokeRequest }>({
    mutationFn: ({ endorsementId, request }) => revokeEndorsement(endorsementId, request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: identityKeys.all });
    },
  });
}

// Recovery mutations
export function useSetRecoveryPolicy() {
  const queryClient = useQueryClient();
  return useMutation<RecoveryPolicyResponse, Error, RecoveryPolicyRequest>({
    mutationFn: setRecoveryPolicy,
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({
        queryKey: identityKeys.recoveryPolicy(variables.account_id),
      });
    },
  });
}

export function useApproveRecovery() {
  return useMutation<RecoveryApprovalResponse, Error, RecoveryApprovalRequest>({
    mutationFn: approveRecovery,
  });
}

export function useRotateRoot() {
  const queryClient = useQueryClient();
  return useMutation<RootRotationResponse, Error, RootRotationRequest>({
    mutationFn: rotateRoot,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: identityKeys.all });
    },
  });
}
