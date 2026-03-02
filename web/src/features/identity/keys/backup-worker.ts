/**
 * Web Worker for backup envelope decryption.
 *
 * Runs Argon2id KDF off the main thread so the UI stays responsive
 * during the multi-second key derivation.
 */
import { decryptBackupEnvelope, DecryptionError } from './crypto';

export interface DecryptRequest {
  type: 'decrypt';
  envelope: Uint8Array;
  password: string;
}

export interface DecryptSuccess {
  type: 'success';
  rootPrivateKey: Uint8Array;
}

export interface DecryptFailure {
  type: 'error';
  message: string;
  isDecryptionError: boolean;
}

export type WorkerResponse = DecryptSuccess | DecryptFailure;

export async function handleDecryptRequest(request: DecryptRequest): Promise<WorkerResponse> {
  const { envelope, password } = request;

  try {
    const rootPrivateKey = await decryptBackupEnvelope(envelope, password);
    return { type: 'success', rootPrivateKey };
  } catch (err) {
    return {
      type: 'error',
      message: err instanceof Error ? err.message : 'Decryption failed',
      isDecryptionError: err instanceof DecryptionError,
    };
  }
}

self.onmessage = async (event: MessageEvent<DecryptRequest>) => {
  const response = await handleDecryptRequest(event.data);
  self.postMessage(response);
};
