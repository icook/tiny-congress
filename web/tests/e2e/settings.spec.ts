import { expect, test } from './fixtures';

test('settings page shows auth warning when not signed in', async ({ page }) => {
  await page.goto('/settings');

  await expect(page.getByRole('heading', { name: /settings/i })).toBeVisible();
  await expect(page.getByText(/sign up or log in to manage devices/i)).toBeVisible();
});

test('settings page shows device list after signup', async ({ page }) => {
  const username = `settings-user-${String(Date.now())}`;

  // Sign up first
  await page.goto('/signup');
  await expect(page.getByLabel(/username/i)).toBeVisible();
  await page.getByLabel(/username/i).fill(username);
  await page.getByRole('button', { name: /sign up/i }).click();
  await expect(page.getByText(/Account Created/i)).toBeVisible({ timeout: 15_000 });

  // Navigate to settings via client-side routing (preserves React state)
  await page.evaluate(() => {
    window.history.pushState({}, '', '/settings');
    window.dispatchEvent(new PopStateEvent('popstate'));
  });

  // Verify settings page loaded with authenticated state
  await expect(page.getByRole('heading', { name: /settings/i })).toBeVisible();
  await expect(page.getByText(/Manage your devices/i)).toBeVisible();

  // Device list should load and show the current device
  await expect(page.getByText(/Current/i)).toBeVisible({ timeout: 10_000 });
  await expect(page.getByText(/Active/i)).toBeVisible();

  // Screenshot: authenticated settings with device list
  await test.info().attach('settings-authenticated', {
    body: await page.screenshot(),
    contentType: 'image/png',
  });
});

test('current device cannot be revoked or renamed', async ({ page }) => {
  const username = `no-self-revoke-${String(Date.now())}`;

  // Sign up
  await page.goto('/signup');
  await expect(page.getByLabel(/username/i)).toBeVisible();
  await page.getByLabel(/username/i).fill(username);
  await page.getByRole('button', { name: /sign up/i }).click();
  await expect(page.getByText(/Account Created/i)).toBeVisible({ timeout: 15_000 });

  // Navigate to settings
  await page.evaluate(() => {
    window.history.pushState({}, '', '/settings');
    window.dispatchEvent(new PopStateEvent('popstate'));
  });

  // Wait for device list to load
  await expect(page.getByText(/Current/i)).toBeVisible({ timeout: 10_000 });

  // The current device row should NOT have rename or revoke action icons
  // With only one device (current), the Actions column should be empty
  await expect(page.getByRole('button', { name: /rename/i })).toBeHidden();
  await expect(page.getByRole('button', { name: /revoke/i })).toBeHidden();
});
