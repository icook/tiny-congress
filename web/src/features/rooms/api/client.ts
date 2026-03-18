/**
 * Rooms API client
 * Type-safe REST client for rooms, polls, and voting endpoints
 */

import { fetchJson } from '@/api/fetchClient';
import { signedFetchJson } from '@/api/signing';
import type { CryptoModule } from '@/providers/CryptoProvider';

// === Types ===

export interface Room {
  id: string;
  name: string;
  description: string | null;
  eligibility_topic: string;
  status: string;
  created_at: string;
}

export interface Poll {
  id: string;
  room_id: string;
  question: string;
  description: string | null;
  status: string;
  created_at: string;
  closes_at: string | null;
}

export interface Evidence {
  id: string;
  stance: 'pro' | 'con';
  claim: string;
  source: string | null;
}

export interface Dimension {
  id: string;
  name: string;
  description: string | null;
  min_value: number;
  max_value: number;
  min_label: string | null;
  max_label: string | null;
  sort_order: number;
  evidence: Evidence[];
}

export interface PollDetail {
  poll: Poll;
  dimensions: Dimension[];
}

export interface DimensionStats {
  dimension_id: string;
  dimension_name: string;
  count: number;
  mean: number;
  median: number;
  stddev: number;
  min: number;
  max: number;
}

export interface PollResults {
  poll: Poll;
  dimensions: DimensionStats[];
  voter_count: number;
}

export interface BucketCount {
  label: string;
  count: number;
}

export interface DimensionDistribution {
  dimension_id: string;
  dimension_name: string;
  buckets: BucketCount[];
}

export interface PollDistribution {
  dimensions: DimensionDistribution[];
}

export interface Vote {
  dimension_id: string;
  value: number;
  updated_at: string;
}

export interface DimensionVote {
  dimension_id: string;
  value: number;
}

// === Public endpoints (no auth) ===

export async function listRooms(): Promise<Room[]> {
  return fetchJson('/rooms');
}

export async function getRoom(roomId: string): Promise<Room> {
  return fetchJson(`/rooms/${roomId}`);
}

export async function listPolls(roomId: string): Promise<Poll[]> {
  return fetchJson(`/rooms/${roomId}/polls`);
}

export async function getAgenda(roomId: string): Promise<Poll[]> {
  return fetchJson(`/rooms/${roomId}/agenda`);
}

export async function getPollDetail(roomId: string, pollId: string): Promise<PollDetail> {
  return fetchJson(`/rooms/${roomId}/polls/${pollId}`);
}

export async function getPollResults(roomId: string, pollId: string): Promise<PollResults> {
  return fetchJson(`/rooms/${roomId}/polls/${pollId}/results`);
}

export async function getPollDistribution(
  roomId: string,
  pollId: string
): Promise<PollDistribution> {
  return fetchJson(`/rooms/${roomId}/polls/${pollId}/results/distribution`);
}

// === Authenticated endpoints ===

export async function castVote(
  roomId: string,
  pollId: string,
  votes: DimensionVote[],
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule
): Promise<Vote[]> {
  return signedFetchJson(
    `/rooms/${roomId}/polls/${pollId}/vote`,
    'POST',
    deviceKid,
    privateKey,
    wasmCrypto,
    { votes }
  );
}

export async function getMyVotes(
  roomId: string,
  pollId: string,
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule
): Promise<Vote[]> {
  return signedFetchJson(
    `/rooms/${roomId}/polls/${pollId}/my-votes`,
    'GET',
    deviceKid,
    privateKey,
    wasmCrypto
  );
}
