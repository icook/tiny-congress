import { afterEach, beforeEach, describe, expect, Mock, test, vi } from 'vitest';
import { fetchJson, signup, type SignupRequest } from './client';

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
});
