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

/** Build metadata exposed via GraphQL and logs. */
export type BuildInfo = {
  buildTime: Scalars['String']['output'];
  gitSha: Scalars['String']['output'];
  message?: Maybe<Scalars['String']['output']>;
  version: Scalars['String']['output'];
};

export type MutationRoot = {
  submitVote: Scalars['Boolean']['output'];
};

export type MutationRootSubmitVoteArgs = {
  choice: Scalars['ID']['input'];
  pairingId: Scalars['ID']['input'];
  userId: Scalars['ID']['input'];
};

export type Pairing = {
  id: Scalars['ID']['output'];
  topicA: Topic;
  topicB: Topic;
};

export type QueryRoot = {
  buildInfo: BuildInfo;
  currentPairing?: Maybe<Pairing>;
  currentRound?: Maybe<Round>;
  topTopics: Array<TopicRanking>;
};

export type QueryRootCurrentPairingArgs = {
  roundId: Scalars['ID']['input'];
};

export type QueryRootTopTopicsArgs = {
  limit?: InputMaybe<Scalars['Int']['input']>;
};

export type Round = {
  endTime: Scalars['String']['output'];
  id: Scalars['ID']['output'];
  startTime: Scalars['String']['output'];
  status: Scalars['String']['output'];
};

export type Topic = {
  description: Scalars['String']['output'];
  id: Scalars['ID']['output'];
  title: Scalars['String']['output'];
};

export type TopicRanking = {
  rank: Scalars['Int']['output'];
  score: Scalars['Float']['output'];
  topic: Topic;
  topicId: Scalars['ID']['output'];
};

type Properties<T> = Required<{
  [K in keyof T]: z.ZodType<T[K]>;
}>;

type definedNonNullAny = {};

export const isDefinedNonNullAny = (v: any): v is definedNonNullAny =>
  v !== undefined && v !== null;

export const definedNonNullAnySchema = z.any().refine((v) => isDefinedNonNullAny(v));

export const RoundSchema: z.ZodObject<Properties<Round>> = z.object({
  __typename: z.literal('Round').optional(),
  endTime: z.string(),
  id: z.string(),
  startTime: z.string(),
  status: z.string(),
});

export const TopicSchema: z.ZodObject<Properties<Topic>> = z.object({
  __typename: z.literal('Topic').optional(),
  description: z.string(),
  id: z.string(),
  title: z.string(),
});

export const TopicRankingSchema: z.ZodObject<Properties<TopicRanking>> = z.object({
  __typename: z.literal('TopicRanking').optional(),
  rank: z.number(),
  score: z.number(),
  topic: z.lazy(() => TopicSchema),
  topicId: z.string(),
});

export const PairingSchema: z.ZodObject<Properties<Pairing>> = z.object({
  __typename: z.literal('Pairing').optional(),
  id: z.string(),
  topicA: z.lazy(() => TopicSchema),
  topicB: z.lazy(() => TopicSchema),
});

export const MutationRootSchema: z.ZodObject<Properties<MutationRoot>> = z.object({
  __typename: z.literal('MutationRoot').optional(),
  submitVote: z.boolean(),
});

export const MutationRootSubmitVoteArgsSchema: z.ZodObject<Properties<MutationRootSubmitVoteArgs>> =
  z.object({
    choice: z.string(),
    pairingId: z.string(),
    userId: z.string(),
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
  currentPairing: z.lazy(() => PairingSchema.nullish()),
  currentRound: z.lazy(() => RoundSchema.nullish()),
  topTopics: z.array(z.lazy(() => TopicRankingSchema)),
});

export const QueryRootCurrentPairingArgsSchema: z.ZodObject<
  Properties<QueryRootCurrentPairingArgs>
> = z.object({
  roundId: z.string(),
});

export const QueryRootTopTopicsArgsSchema: z.ZodObject<Properties<QueryRootTopTopicsArgs>> =
  z.object({
    limit: z.number().nullish(),
  });
