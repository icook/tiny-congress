import { expect, type Page } from './fixtures';

/**
 * Sign up a new user and wait for the success screen.
 * Returns the username used.
 */
export async function signupUser(
  page: Page,
  username?: string,
  password = 'test-password-123'
): Promise<string> {
  const name = username ?? `test-user-${String(Date.now())}`;
  await page.goto('/signup');
  await expect(page.getByLabel(/username/i)).toBeVisible();
  await page.getByLabel(/username/i).fill(name);
  await page.getByLabel(/^Backup Password/i).fill(password);
  await page.getByLabel(/^Confirm Backup Password/i).fill(password);
  await page.getByRole('button', { name: /sign up/i }).click();
  await expect(page.getByText(/Welcome/i)).toBeVisible({ timeout: 15_000 });
  return name;
}

/**
 * Make a signed API call using the current browser session's device credentials.
 * Must be called after signupUser() when a device key is stored in IndexedDB.
 */
export async function signedApiCall(
  page: Page,
  method: string,
  path: string,
  body?: unknown
): Promise<unknown> {
  return page.evaluate(
    async ({ method, path, body }) => {
      // ── Read device credentials from IndexedDB ──
      const db: IDBDatabase = await new Promise((resolve, reject) => {
        const req = indexedDB.open('tc-device-store', 2);
        req.onsuccess = () => {
          resolve(req.result);
        };
        req.onerror = () => {
          reject(new Error('Failed to open tc-device-store'));
        };
      });

      const device: { kid: string; privateKey: CryptoKey } | undefined = await new Promise(
        (resolve, reject) => {
          const tx = db.transaction('device', 'readonly');
          const store = tx.objectStore('device');
          const req = store.get('current');
          req.onsuccess = () => {
            resolve(req.result as { kid: string; privateKey: CryptoKey } | undefined);
          };
          req.onerror = () => {
            reject(new Error('Failed to read device from IndexedDB'));
          };
        }
      );

      if (!device) {
        throw new Error('No device credentials in IndexedDB');
      }

      // ── SHA-256 hash of body ──
      const bodyStr = body !== undefined ? JSON.stringify(body) : '';
      const bodyBytes = new TextEncoder().encode(bodyStr);
      const hashBuf = await crypto.subtle.digest('SHA-256', bodyBytes);
      const bodyHash = Array.from(new Uint8Array(hashBuf))
        .map((b) => b.toString(16).padStart(2, '0'))
        .join('');

      // ── Build canonical signing string ──
      const timestamp = Math.floor(Date.now() / 1000).toString();
      const nonce = crypto.randomUUID();
      const canonical = `${method}\n${path}\n${timestamp}\n${nonce}\n${bodyHash}`;

      // ── Sign with Ed25519 ──
      const sigBuf = await crypto.subtle.sign(
        'Ed25519',
        device.privateKey,
        new TextEncoder().encode(canonical)
      );

      // ── Base64url encode signature ──
      const b64url = btoa(String.fromCharCode(...new Uint8Array(sigBuf)))
        .replace(/\+/g, '-')
        .replace(/\//g, '_')
        .replace(/=+$/, '');

      // ── Resolve API base URL (mirrors app's getApiBaseUrl) ──
      const apiUrl =
        (window as unknown as { __TC_ENV__?: { VITE_API_URL?: string } }).__TC_ENV__
          ?.VITE_API_URL ?? 'http://localhost:8080';

      const headers: Record<string, string> = {
        'Content-Type': 'application/json',
        'X-Device-Kid': device.kid,
        'X-Signature': b64url,
        'X-Timestamp': timestamp,
        'X-Nonce': nonce,
      };

      const options: RequestInit = { method, headers };
      if (body !== undefined) {
        options.body = bodyStr;
      }

      const response = await fetch(`${apiUrl}${path}`, options);
      if (!response.ok) {
        const text = await response.text();
        throw new Error(`API ${method} ${path} failed (${String(response.status)}): ${text}`);
      }

      return response.json() as Promise<unknown>;
    },
    { method, path, body }
  );
}

/**
 * Seed a room with an active poll and one dimension.
 * The current user (from signupUser) becomes the room owner and can vote.
 */
export async function seedRoomWithPoll(page: Page): Promise<{ roomId: string; pollId: string }> {
  const room = (await signedApiCall(page, 'POST', '/rooms', {
    name: 'E2E Test Room',
    description: 'Auto-created for E2E tests',
  })) as { id: string };

  const poll = (await signedApiCall(page, 'POST', `/rooms/${room.id}/polls`, {
    question: 'Should we increase park funding?',
    description: 'Annual budget allocation for city parks and green spaces',
  })) as { id: string };

  await signedApiCall(page, 'POST', `/rooms/${room.id}/polls/${poll.id}/dimensions`, {
    name: 'Funding Level',
    description: 'How much should the city invest in park maintenance?',
    min_value: 0,
    max_value: 1,
    min_label: 'Decrease funding',
    max_label: 'Increase funding',
    sort_order: 0,
  });

  await signedApiCall(page, 'POST', `/rooms/${room.id}/polls/${poll.id}/status`, {
    status: 'active',
  });

  return { roomId: room.id, pollId: poll.id };
}
