import { expect, test } from './fixtures';

const PASSWORD = 'test-password-123';

/**
 * Sign up a user on the given page.
 */
async function signupUser(page: import('@playwright/test').Page, username: string) {
  await page.goto('/signup');
  await expect(page.getByLabel(/username/i)).toBeVisible();

  await page.getByLabel(/username/i).fill(username);
  await page.getByLabel('Backup Password', { exact: true }).fill(PASSWORD);
  await page.getByLabel('Confirm Backup Password', { exact: true }).fill(PASSWORD);
  await page.getByRole('button', { name: /sign up/i }).click();
  await expect(page.getByText(/account has been created/i)).toBeVisible({ timeout: 15_000 });
}

/**
 * Create a fresh browser context that inherits the project's baseURL.
 * A new context has empty IndexedDB, simulating a new device / fresh browser
 * without needing to delete databases (which races on webkit).
 */
async function newDeviceContext(browser: import('@playwright/test').Browser) {
  const { baseURL } = test.info().project.use;
  const ctx = await browser.newContext({ baseURL });
  return { ctx, page: await ctx.newPage() };
}

test('login flow recovers account and shows device list', async ({ page, browser }) => {
  const username = `login-user-${String(Date.now())}`;
  await signupUser(page, username);

  // Fresh context = new device with no IDB data.
  const device2 = await newDeviceContext(browser);

  try {
    await device2.page.goto('/login');
    await expect(device2.page.getByLabel(/username/i)).toBeVisible();

    await test.info().attach('login-form', {
      body: await device2.page.screenshot(),
      contentType: 'image/png',
    });

    await device2.page.getByLabel(/username/i).fill(username);
    await device2.page.getByLabel(/backup password/i).fill(PASSWORD);
    await device2.page.getByRole('button', { name: /log in/i }).click();

    // Argon2id KDF with m_cost=65536 can take several seconds in the browser.
    // After decryption, the login API call creates a device and navigates to /rooms.
    await expect(device2.page.getByRole('heading', { name: /rooms/i })).toBeVisible({
      timeout: 30_000,
    });

    // Navigate to settings to verify device list
    await device2.page.goto('/settings');
    await expect(device2.page.getByRole('heading', { name: /settings/i })).toBeVisible();
    await expect(device2.page.getByText(/Manage your devices/i)).toBeVisible();

    // Device list should load with two devices: the signup device + the login device
    await expect(device2.page.getByText(/Current/i)).toBeVisible({ timeout: 10_000 });
    // Both devices show "Active" badge — verify at least one is visible
    await expect(device2.page.getByText(/Active/i).first()).toBeVisible();

    await test.info().attach('login-settings', {
      body: await device2.page.screenshot(),
      contentType: 'image/png',
    });
  } finally {
    await device2.ctx.close();
  }
});

test('login with wrong password shows error', async ({ page, browser }) => {
  const username = `login-badpw-${String(Date.now())}`;
  await signupUser(page, username);

  const device2 = await newDeviceContext(browser);

  try {
    await device2.page.goto('/login');
    await expect(device2.page.getByLabel(/username/i)).toBeVisible();

    await device2.page.getByLabel(/username/i).fill(username);
    await device2.page.getByLabel(/backup password/i).fill('wrong-password');
    await device2.page.getByRole('button', { name: /log in/i }).click();

    // Decryption with the wrong password should fail and show an error
    await expect(device2.page.getByText(/Wrong password or corrupted backup/i)).toBeVisible({
      timeout: 30_000,
    });

    // Should still be on the login page (not navigated away)
    await expect(device2.page.getByRole('heading', { name: /log in/i })).toBeVisible();
  } finally {
    await device2.ctx.close();
  }
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
