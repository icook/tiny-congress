/**
 * TanStack Query hooks for identity API
 */

import { useMutation } from '@tanstack/react-query';
import { signup, type SignupRequest, type SignupResponse } from './client';

/**
 * Mutation hook for user signup
 */
export function useSignup() {
  return useMutation<SignupResponse, Error, SignupRequest>({
    mutationFn: signup,
  });
}
