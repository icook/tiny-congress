import { expect, test } from './fixtures';

const PASSWORD = 'test-password-123';

/**
 * Sign up a user and clear device state so subsequent login tests
 * simulate a fresh browser / new device.
 */
async function signupAndClearDevice(page: import('@playwright/test').Page, username: string) {
  await page.goto('/signup');
  await expect(page.getByLabel(/username/i)).toBeVisible();

  await page.getByLabel(/username/i).fill(username);
  await page.getByLabel(/backup password/i).fill(PASSWORD);
  await page.getByRole('button', { name: /sign up/i }).click();
  await expect(page.getByText(/Account Created/i)).toBeVisible({ timeout: 15_000 });

  // Clear IndexedDB to simulate a new device / fresh browser
  await page.evaluate(() => indexedDB.deleteDatabase('tc-device-store'));
}

test('login flow recovers account and shows device list', async ({ page }) => {
  const username = `login-user-${String(Date.now())}`;
  await signupAndClearDevice(page, username);

  // Navigate to login
  await page.goto('/login');
  await expect(page.getByLabel(/username/i)).toBeVisible();

  // Screenshot: login form
  await test.info().attach('login-form', {
    body: await page.screenshot(),
    contentType: 'image/png',
  });

  // Fill and submit
  await page.getByLabel(/username/i).fill(username);
  await page.getByLabel(/backup password/i).fill(PASSWORD);
  await page.getByRole('button', { name: /log in/i }).click();

  // Argon2id KDF with m_cost=65536 can take several seconds in the browser.
  // After decryption, the login API call creates a device and navigates to /settings.
  await expect(page.getByRole('heading', { name: /settings/i })).toBeVisible({ timeout: 30_000 });
  await expect(page.getByText(/Manage your devices/i)).toBeVisible();

  // Device list should load with two devices: the signup device + the login device
  await expect(page.getByText(/Current/i)).toBeVisible({ timeout: 10_000 });
  // Both devices show "Active" badge — verify at least one is visible
  await expect(page.getByText(/Active/i).first()).toBeVisible();

  // Screenshot: settings with device list after login
  await test.info().attach('login-settings', {
    body: await page.screenshot(),
    contentType: 'image/png',
  });
});

test('login with wrong password shows error', async ({ page }) => {
  const username = `login-badpw-${String(Date.now())}`;
  await signupAndClearDevice(page, username);

  await page.goto('/login');
  await expect(page.getByLabel(/username/i)).toBeVisible();

  await page.getByLabel(/username/i).fill(username);
  await page.getByLabel(/backup password/i).fill('wrong-password');
  await page.getByRole('button', { name: /log in/i }).click();

  // Decryption with the wrong password should fail and show an error
  await expect(page.getByText(/Wrong password or corrupted backup/i)).toBeVisible({
    timeout: 30_000,
  });

  // Should still be on the login page (not navigated away)
  await expect(page.getByRole('heading', { name: /log in/i })).toBeVisible();
});

test('login with unknown username shows error', async ({ page }) => {
  await page.goto('/login');
  await expect(page.getByLabel(/username/i)).toBeVisible();

  await page.getByLabel(/username/i).fill('nobody-exists-here');
  await page.getByLabel(/backup password/i).fill('any-password');
  await page.getByRole('button', { name: /log in/i }).click();

  // fetchBackup should 404 and surface an error
  await expect(page.getByText(/Login failed/i)).toBeVisible({ timeout: 10_000 });
});
