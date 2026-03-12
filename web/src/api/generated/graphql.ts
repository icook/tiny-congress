import * as z from 'zod';

export type Maybe<T> = T | null;
export type InputMaybe<T> = Maybe<T>;
export type Exact<T extends { [key: string]: unknown }> = { [K in keyof T]: T[K] };
export type MakeOptional<T, K extends keyof T> = Omit<T, K> & { [SubKey in K]?: Maybe<T[SubKey]> };
export type MakeMaybe<T, K extends keyof T> = Omit<T, K> & { [SubKey in K]: Maybe<T[SubKey]> };
export type MakeEmpty<T extends { [key: string]: unknown }, K extends keyof T> = {
  [_ in K]?: never;
};
export type Incremental<T> =
  | T
  | { [P in keyof T]?: P extends ' $fragmentName' | '__typename' ? T[P] : never };
/** All built-in and custom scalars, mapped to their actual values */
export type Scalars = {
  ID: { input: string; output: string };
  String: { input: string; output: string };
  Boolean: { input: boolean; output: boolean };
  Int: { input: number; output: number };
  Float: { input: number; output: number };
};

/**
 * Build metadata exposed via GraphQL, REST, and logs.
 *
 * Loaded from environment variables at startup (see [`BuildInfo::from_env`]).
 * These are typically set by the CI pipeline or Dockerfile at image build time.
 */
export type BuildInfo = {
  /**
   * Build timestamp in RFC 3339 format. Read from `BUILD_TIME` env var.
   * Defaults to `"unknown"`.
   */
  buildTime: Scalars['String']['output'];
  /** Git commit SHA. Read from `GIT_SHA` env var. Defaults to `"unknown"`. */
  gitSha: Scalars['String']['output'];
  /** Optional build message (e.g., CI run URL). Read from `BUILD_MESSAGE` env var. */
  message?: Maybe<Scalars['String']['output']>;
  /**
   * Application version string. Read from `APP_VERSION` or `VERSION` env var.
   * Defaults to `"dev"`.
   */
  version: Scalars['String']['output'];
};

export type MutationRoot = {
  /**
   * Placeholder mutation - returns the input string
   *
   * This exists because GraphQL requires at least one mutation.
   * Replace with actual mutations as features are implemented.
   */
  echo: Scalars['String']['output'];
};

export type MutationRootEchoArgs = {
  message: Scalars['String']['input'];
};

export type QueryRoot = {
  /** Returns build metadata for the running service */
  buildInfo: BuildInfo;
};

type Properties<T> = Required<{
  [K in keyof T]: z.ZodType<T[K]>;
}>;

type definedNonNullAny = {};

export const isDefinedNonNullAny = (v: any): v is definedNonNullAny =>
  v !== undefined && v !== null;

export const definedNonNullAnySchema = z.any().refine((v) => isDefinedNonNullAny(v));

export const MutationRootSchema: z.ZodObject<Properties<MutationRoot>> = z.object({
  __typename: z.literal('MutationRoot').optional(),
  echo: z.string(),
});

export const MutationRootEchoArgsSchema: z.ZodObject<Properties<MutationRootEchoArgs>> = z.object({
  message: z.string(),
});

export const BuildInfoSchema: z.ZodObject<Properties<BuildInfo>> = z.object({
  __typename: z.literal('BuildInfo').optional(),
  buildTime: z.string(),
  gitSha: z.string(),
  message: z.string().nullish(),
  version: z.string(),
});

export const QueryRootSchema: z.ZodObject<Properties<QueryRoot>> = z.object({
  __typename: z.literal('QueryRoot').optional(),
  buildInfo: z.lazy(() => BuildInfoSchema),
});
