import { afterEach, beforeEach, describe, expect, Mock, test, vi } from 'vitest';
import type { CryptoModule } from '@/providers/CryptoProvider';
import {
  fetchJson,
  listDevices,
  login,
  renameDevice,
  revokeDevice,
  signup,
  type SignupRequest,
} from './client';

function headersOf(mockFetch: Mock): Record<string, string> {
  const call = mockFetch.mock.calls[0] as [string, RequestInit];
  const h = new Headers(call[1].headers);
  const out: Record<string, string> = {};
  h.forEach((v, k) => {
    out[k] = v;
  });
  return out;
}

function makeSignupRequest(overrides?: Partial<SignupRequest>): SignupRequest {
  return {
    username: 'alice',
    root_pubkey: 'mock-key',
    backup: { encrypted_blob: 'mock-backup' },
    device: { pubkey: 'mock-device-key', name: 'Test Device', certificate: 'mock-cert' },
    ...overrides,
  };
}

describe('identity api client', () => {
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn());
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  test('posts signup request and returns parsed payload', async () => {
    const responseBody = { account_id: 'abc', root_kid: 'kid-123', device_kid: 'dev-456' };
    (fetch as unknown as Mock).mockResolvedValue({
      ok: true,
      status: 201,
      statusText: 'Created',
      json: vi.fn().mockResolvedValue(responseBody),
      headers: {},
    });

    const req = makeSignupRequest();
    const result = await signup(req);

    expect(fetch).toHaveBeenCalledWith(
      expect.stringContaining('/auth/signup'),
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify(req),
      })
    );
    expect(headersOf(fetch as unknown as Mock)).toEqual({
      'content-type': 'application/json',
    });
    expect(result).toEqual(responseBody);
  });

  test('throws with server-provided error message on failure', async () => {
    (fetch as unknown as Mock).mockResolvedValue({
      ok: false,
      status: 500,
      statusText: 'Server Error',
      json: vi.fn().mockResolvedValue({ error: 'boom' }),
      headers: {},
    });

    await expect(signup(makeSignupRequest({ username: 'bob' }))).rejects.toThrow('boom');
  });

  test('falls back to HTTP status when error body has no error field', async () => {
    (fetch as unknown as Mock).mockResolvedValue({
      ok: false,
      status: 404,
      statusText: 'Not Found',
      json: vi.fn().mockResolvedValue({}),
      headers: {},
    });

    await expect(signup(makeSignupRequest({ username: 'bob' }))).rejects.toThrow(
      'HTTP 404: Not Found'
    );
  });

  test('preserves caller-provided headers alongside default Content-Type', async () => {
    (fetch as unknown as Mock).mockResolvedValue({
      ok: true,
      status: 200,
      statusText: 'OK',
      json: vi.fn().mockResolvedValue({ ok: true }),
      headers: {},
    });

    await fetchJson('/test', {
      headers: { Authorization: 'Bearer token-123' },
    });

    expect(headersOf(fetch as unknown as Mock)).toEqual({
      'content-type': 'application/json',
      authorization: 'Bearer token-123',
    });
  });

  test('allows caller to override Content-Type', async () => {
    (fetch as unknown as Mock).mockResolvedValue({
      ok: true,
      status: 200,
      statusText: 'OK',
      json: vi.fn().mockResolvedValue({ ok: true }),
      headers: {},
    });

    await fetchJson('/test', {
      headers: { 'Content-Type': 'text/plain' },
    });

    expect(headersOf(fetch as unknown as Mock)).toEqual({
      'content-type': 'text/plain',
    });
  });

  test('handles JSON parse failure gracefully', async () => {
    (fetch as unknown as Mock).mockResolvedValue({
      ok: false,
      status: 500,
      statusText: 'Internal Server Error',
      json: vi.fn().mockRejectedValue(new Error('Invalid JSON')),
      headers: {},
    });

    await expect(signup(makeSignupRequest({ username: 'bob' }))).rejects.toThrow('Unknown error');
  });

  test('handles 204 No Content response', async () => {
    (fetch as unknown as Mock).mockResolvedValue({
      ok: true,
      status: 204,
      statusText: 'No Content',
      headers: {},
    });

    const result = await fetchJson('/test', { method: 'DELETE' });
    expect(result).toBeUndefined();
  });
});

describe('signed device API', () => {
  const mockCrypto: CryptoModule = {
    derive_kid: vi.fn(),
    encode_base64url: vi.fn((bytes: Uint8Array) => Buffer.from(bytes).toString('base64url')),
    decode_base64url: vi.fn(),
  };
  const deviceKid = 'test-device-kid';
  // Ed25519 private key (32 random bytes)
  const privateKey = new Uint8Array(32).fill(42);

  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn());
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  test('listDevices sends GET with auth headers', async () => {
    const devices = {
      devices: [
        {
          device_kid: 'kid1',
          device_name: 'Dev 1',
          created_at: '2026-01-01',
          last_used_at: null,
          revoked_at: null,
        },
      ],
    };
    (fetch as unknown as Mock).mockResolvedValue({
      ok: true,
      status: 200,
      statusText: 'OK',
      json: vi.fn().mockResolvedValue(devices),
      headers: {},
    });

    const result = await listDevices(deviceKid, privateKey, mockCrypto);

    const call = (fetch as unknown as Mock).mock.calls[0] as [string, RequestInit];
    const headers = new Headers(call[1].headers);
    expect(headers.get('X-Device-Kid')).toBe(deviceKid);
    expect(headers.get('X-Signature')).toBeTruthy();
    expect(headers.get('X-Timestamp')).toBeTruthy();
    expect(headers.get('X-Nonce')).toBeTruthy();
    expect(call[1].method).toBe('GET');
    expect(result).toEqual(devices);
  });

  test('revokeDevice sends DELETE to correct path', async () => {
    (fetch as unknown as Mock).mockResolvedValue({
      ok: true,
      status: 204,
      statusText: 'No Content',
      headers: {},
    });

    await revokeDevice('target-kid', deviceKid, privateKey, mockCrypto);

    const call = (fetch as unknown as Mock).mock.calls[0] as [string, RequestInit];
    expect(call[0]).toContain('/auth/devices/target-kid');
    expect(call[1].method).toBe('DELETE');
  });

  test('renameDevice sends PATCH with name in body', async () => {
    (fetch as unknown as Mock).mockResolvedValue({
      ok: true,
      status: 204,
      statusText: 'No Content',
      headers: {},
    });

    await renameDevice('target-kid', 'New Name', deviceKid, privateKey, mockCrypto);

    const call = (fetch as unknown as Mock).mock.calls[0] as [string, RequestInit];
    expect(call[0]).toContain('/auth/devices/target-kid');
    expect(call[1].method).toBe('PATCH');
    expect(call[1].body).toBe(JSON.stringify({ name: 'New Name' }));
  });

  test('posts login request with timestamp and device payload', async () => {
    const responseBody = { account_id: 'abc', root_kid: 'kid-1', device_kid: 'kid-2' };
    (fetch as unknown as Mock).mockResolvedValue({
      ok: true,
      status: 200,
      statusText: 'OK',
      json: vi.fn().mockResolvedValue(responseBody),
      headers: {},
    });

    const result = await login({
      username: 'alice',
      timestamp: 1700000000,
      device: {
        pubkey: 'mock-pubkey',
        name: 'Test Device',
        certificate: 'mock-cert',
      },
    });

    expect(fetch).toHaveBeenCalledWith(
      expect.stringContaining('/auth/login'),
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({
          username: 'alice',
          timestamp: 1700000000,
          device: {
            pubkey: 'mock-pubkey',
            name: 'Test Device',
            certificate: 'mock-cert',
          },
        }),
      })
    );
    expect(result).toEqual(responseBody);
  });
});
