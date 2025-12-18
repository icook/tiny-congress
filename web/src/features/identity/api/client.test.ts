import { afterEach, beforeEach, describe, expect, Mock, test, vi } from 'vitest';
import { signup } from './client';

describe('identity api client', () => {
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn());
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  test('posts signup request and returns parsed payload', async () => {
    const responseBody = { account_id: 'abc', root_kid: 'kid-123' };
    (fetch as unknown as Mock).mockResolvedValue({
      ok: true,
      status: 201,
      statusText: 'Created',
      json: vi.fn().mockResolvedValue(responseBody),
      headers: {},
    });

    const result = await signup({ username: 'alice', root_pubkey: 'mock-key' });

    expect(fetch).toHaveBeenCalledWith(
      expect.stringContaining('/auth/signup'),
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ username: 'alice', root_pubkey: 'mock-key' }),
        headers: expect.objectContaining({ 'Content-Type': 'application/json' }),
      })
    );
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

    await expect(signup({ username: 'bob', root_pubkey: 'key' })).rejects.toThrow('boom');
  });
});
