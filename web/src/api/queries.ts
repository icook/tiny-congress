/**
 * TanStack Query option factories
 * Centralized query definitions for consistent caching and refetching behavior
 */

import { queryOptions } from '@tanstack/react-query';
import { fetchBuildInfo } from './buildInfo';

export const buildInfoQuery = queryOptions({
  queryKey: ['build-info'],
  queryFn: fetchBuildInfo,
  staleTime: 60 * 60 * 1000, // 1 hour - build info rarely changes
  gcTime: Infinity, // Keep forever - it's small and static
});
