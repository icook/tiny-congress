/**
 * Rooms API client
 * Type-safe REST client for rooms, polls, and voting endpoints
 */

import { fetchJson } from '@/api/fetchClient';
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
}

export interface Dimension {
  id: string;
  name: string;
  description: string | null;
  min_value: number;
  max_value: number;
  sort_order: number;
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

export interface Vote {
  dimension_id: string;
  value: number;
  updated_at: string;
}

export interface DimensionVote {
  dimension_id: string;
  value: number;
}

export interface HasEndorsementResponse {
  has_endorsement: boolean;
}

// === Signing helpers (inline to avoid cross-feature imports) ===

async function sha256Hex(data: Uint8Array): Promise<string> {
  const hash = await globalThis.crypto.subtle.digest(
    'SHA-256',
    data as ArrayBufferView<ArrayBuffer>
  );
  return Array.from(new Uint8Array(hash))
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('');
}

async function signedFetch<T>(
  path: string,
  method: string,
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule,
  body?: unknown
): Promise<T> {
  const bodyStr = body !== undefined ? JSON.stringify(body) : '';
  const bodyBytes = new TextEncoder().encode(bodyStr);

  const timestamp = Math.floor(Date.now() / 1000).toString();
  const nonce = globalThis.crypto.randomUUID();
  const bodyHash = await sha256Hex(bodyBytes);
  const canonical = `${method}\n${path}\n${timestamp}\n${nonce}\n${bodyHash}`;

  const signatureBuffer = await globalThis.crypto.subtle.sign(
    'Ed25519',
    privateKey,
    new TextEncoder().encode(canonical) as ArrayBufferView<ArrayBuffer>
  );

  const authHeaders: Record<string, string> = {
    'X-Device-Kid': deviceKid,
    'X-Signature': wasmCrypto.encode_base64url(new Uint8Array(signatureBuffer)),
    'X-Timestamp': timestamp,
    'X-Nonce': nonce,
  };

  const options: RequestInit = {
    method,
    headers: authHeaders,
  };

  if (body !== undefined) {
    options.body = bodyStr;
  }

  return fetchJson<T>(path, options);
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

export async function getPollDetail(roomId: string, pollId: string): Promise<PollDetail> {
  return fetchJson(`/rooms/${roomId}/polls/${pollId}`);
}

export async function getPollResults(roomId: string, pollId: string): Promise<PollResults> {
  return fetchJson(`/rooms/${roomId}/polls/${pollId}/results`);
}

export async function checkEndorsement(
  subjectId: string,
  topic: string
): Promise<HasEndorsementResponse> {
  return fetchJson(
    `/endorsements/check?subject_id=${subjectId}&topic=${encodeURIComponent(topic)}`
  );
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
  return signedFetch(
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
  return signedFetch(
    `/rooms/${roomId}/polls/${pollId}/my-votes`,
    'GET',
    deviceKid,
    privateKey,
    wasmCrypto
  );
}
