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

self.onmessage = async (event: MessageEvent<DecryptRequest>) => {
  const { envelope, password } = event.data;

  try {
    const rootPrivateKey = await decryptBackupEnvelope(envelope, password);
    const response: DecryptSuccess = {
      type: 'success',
      rootPrivateKey,
    };
    self.postMessage(response);
  } catch (err) {
    const response: DecryptFailure = {
      type: 'error',
      message: err instanceof Error ? err.message : 'Decryption failed',
      isDecryptionError: err instanceof DecryptionError,
    };
    self.postMessage(response);
  }
};
