/**
 * Zod schemas for runtime API response validation
 *
 * Validates external data from API responses to ensure type safety at runtime.
 * @see docs/interfaces/react-coding-standards.md
 */

import { z } from 'zod';

/**
 * Build information schema
 */
export const buildInfoSchema = z.object({
  version: z.string(),
  gitSha: z.string(),
  buildTime: z.string(),
  message: z.string().nullable().optional(),
});

export type BuildInfo = z.infer<typeof buildInfoSchema>;

/**
 * GraphQL response wrapper for BuildInfo query
 */
export const buildInfoQueryResultSchema = z.object({
  buildInfo: buildInfoSchema,
});

export type BuildInfoQueryResult = z.infer<typeof buildInfoQueryResultSchema>;
