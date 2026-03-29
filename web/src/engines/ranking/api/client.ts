/**
 * Ranking engine API client
 * Type-safe REST client for ranking rounds, submissions, matchups, and leaderboard endpoints
 */

import { fetchJson } from '@/api/fetchClient';
import { signedFetchFormData, signedFetchJson } from '@/api/signing';
import type { CryptoModule } from '@/providers/CryptoProvider';

// === Types ===

export interface Round {
  id: string;
  room_id: string;
  round_number: number;
  submit_opens_at: string;
  rank_opens_at: string;
  closes_at: string;
  status: 'submitting' | 'ranking' | 'closed';
}

export interface Submission {
  id: string;
  round_id: string;
  author_id?: string;
  content_type: 'url' | 'image';
  url?: string;
  image_key?: string;
  caption?: string;
  created_at: string;
}

export interface MatchupPair {
  submission_a: Submission;
  submission_b: Submission;
}

export interface MatchupResult {
  id: string;
  winner_id?: string;
  created_at: string;
}

export interface LeaderboardEntry {
  submission: Submission;
  rating: number;
  deviation: number;
  matchup_count: number;
  rank: number;
}

export interface LeaderboardResponse {
  round_id: string;
  round_status: string;
  entries: LeaderboardEntry[];
}

export interface HallOfFameEntry {
  submission: Submission & { author_id: string };
  round_number: number;
  final_rating: number;
  rank: number;
}

export interface SubmitBody {
  content_type: string;
  url?: string;
  image_key?: string;
  caption?: string;
}

export interface RecordMatchupBody {
  winner_id?: string;
  loser_id?: string;
  submission_a?: string;
  submission_b?: string;
  skipped?: boolean;
}

// === Public endpoints (no auth) ===

export async function getCurrentRounds(roomId: string): Promise<Round[]> {
  return fetchJson(`/api/v1/rooms/${roomId}/rounds/current`);
}

export async function listRounds(roomId: string): Promise<Round[]> {
  return fetchJson(`/api/v1/rooms/${roomId}/rounds`);
}

export async function getLeaderboard(
  roomId: string,
  roundId: string
): Promise<LeaderboardResponse> {
  return fetchJson(`/api/v1/rooms/${roomId}/rounds/${roundId}/leaderboard`);
}

export async function getHallOfFame(
  roomId: string,
  limit = 20,
  offset = 0
): Promise<HallOfFameEntry[]> {
  return fetchJson(
    `/api/v1/rooms/${roomId}/hall-of-fame?limit=${String(limit)}&offset=${String(offset)}`
  );
}

// === Authenticated endpoints ===

export async function submitMeme(
  roomId: string,
  body: SubmitBody,
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule
): Promise<Submission> {
  return signedFetchJson(
    `/api/v1/rooms/${roomId}/submissions`,
    'POST',
    deviceKid,
    privateKey,
    wasmCrypto,
    body
  );
}

export async function submitMemeWithImage(
  roomId: string,
  image: File,
  caption: string | undefined,
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule
): Promise<Submission> {
  const formData = new FormData();
  formData.append('image', image);
  if (caption) {
    formData.append('caption', caption);
  }
  return signedFetchFormData(
    `/api/v1/rooms/${roomId}/submissions/upload`,
    'POST',
    deviceKid,
    privateKey,
    wasmCrypto,
    formData
  );
}

export async function getMatchup(
  roomId: string,
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule
): Promise<MatchupPair> {
  return signedFetchJson(
    `/api/v1/rooms/${roomId}/matchup`,
    'GET',
    deviceKid,
    privateKey,
    wasmCrypto
  );
}

export async function recordMatchup(
  roomId: string,
  body: RecordMatchupBody,
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule
): Promise<MatchupResult> {
  return signedFetchJson(
    `/api/v1/rooms/${roomId}/matchups`,
    'POST',
    deviceKid,
    privateKey,
    wasmCrypto,
    body
  );
}
