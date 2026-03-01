/**
 * Decrypt a backup envelope in a web worker.
 *
 * Spawns a worker to run Argon2id KDF off the main thread,
 * keeping the UI responsive during the multi-second computation.
 */
import type { DecryptRequest, WorkerResponse } from './backup-worker';
import { DecryptionError } from './crypto';

/**
 * Decrypt a backup envelope using a web worker for KDF computation.
 *
 * @param envelope - Binary envelope bytes (from server)
 * @param password - User's backup password
 * @returns 32-byte Ed25519 private key
 * @throws DecryptionError if password is wrong or envelope is corrupt
 * @throws Error for other failures
 */
export function decryptBackupInWorker(envelope: Uint8Array, password: string): Promise<Uint8Array> {
  return new Promise((resolve, reject) => {
    const worker = new Worker(new URL('./backup-worker.ts', import.meta.url), { type: 'module' });

    worker.onmessage = (event: MessageEvent<WorkerResponse>) => {
      worker.terminate();
      const response = event.data;

      if (response.type === 'success') {
        resolve(response.rootPrivateKey);
      } else if (response.isDecryptionError) {
        reject(new DecryptionError(response.message));
      } else {
        reject(new Error(response.message));
      }
    };

    worker.onerror = (event) => {
      worker.terminate();
      reject(new Error(`Worker error: ${event.message}`));
    };

    const request: DecryptRequest = { type: 'decrypt', envelope, password };
    worker.postMessage(request);
  });
}
