import { describe, expect, test, vi } from 'vitest';
import { handleDecryptRequest, type DecryptRequest } from './backup-worker';
import { decryptBackupEnvelope, DecryptionError } from './crypto';

vi.mock('./crypto', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./crypto')>();
  return {
    ...actual,
    decryptBackupEnvelope: vi.fn(),
  };
});

const mockDecrypt = vi.mocked(decryptBackupEnvelope);

function makeRequest(overrides?: Partial<DecryptRequest>): DecryptRequest {
  return {
    type: 'decrypt',
    envelope: new Uint8Array([1, 2, 3]),
    password: 'test-password',
    ...overrides,
  };
}

describe('backup worker handler', () => {
  test('success path returns rootPrivateKey', async () => {
    const fakeKey = new Uint8Array([10, 20, 30]);
    mockDecrypt.mockResolvedValue(fakeKey);

    const result = await handleDecryptRequest(makeRequest());

    expect(result).toEqual({ type: 'success', rootPrivateKey: fakeKey });
  });

  test('DecryptionError returns isDecryptionError true', async () => {
    mockDecrypt.mockRejectedValue(new DecryptionError('Wrong password'));

    const result = await handleDecryptRequest(makeRequest());

    expect(result).toEqual({
      type: 'error',
      message: 'Wrong password',
      isDecryptionError: true,
    });
  });

  test('generic error returns isDecryptionError false', async () => {
    mockDecrypt.mockRejectedValue(new Error('Something broke'));

    const result = await handleDecryptRequest(makeRequest());

    expect(result).toEqual({
      type: 'error',
      message: 'Something broke',
      isDecryptionError: false,
    });
  });
});
