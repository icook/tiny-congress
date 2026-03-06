import { expect, test } from './fixtures';

test('signup flow creates account with device key @smoke', async ({ page }) => {
  const username = `test-user-${String(Date.now())}`;

  await page.goto('/signup');

  // Wait for form to be ready
  await expect(page.getByLabel(/username/i)).toBeVisible();

  // Screenshot 1: Initial form
  await test.info().attach('signup-form', {
    body: await page.screenshot(),
    contentType: 'image/png',
  });

  // Fill and submit
  await page.getByLabel(/username/i).fill(username);
  await page.getByLabel('Backup Password', { exact: true }).fill('test-password-123');
  await page.getByLabel('Confirm Backup Password', { exact: true }).fill('test-password-123');
  await page.getByRole('button', { name: /sign up/i }).click();

  // Wait for success — timeout accounts for WASM loading + key generation + API call
  await expect(page.getByText(/Account Created/i)).toBeVisible({ timeout: 15_000 });

  // Verify post-signup UX shows next steps
  await expect(page.getByText(/What's next/i)).toBeVisible();
  await expect(page.getByRole('link', { name: /Browse Rooms/i })).toBeVisible();

  // Verify the session storage message
  await expect(
    page.getByText(/keys were generated locally and stored in this browser session/i)
  ).toBeVisible();

  // Screenshot 2: Success state
  await test.info().attach('signup-success', {
    body: await page.screenshot(),
    contentType: 'image/png',
  });
});

test('signup shows error for duplicate username @smoke', async ({ page, browser }) => {
  const username = `dup-user-${String(Date.now())}`;

  await page.goto('/signup');
  await expect(page.getByLabel(/username/i)).toBeVisible();

  // First signup should succeed
  await page.getByLabel(/username/i).fill(username);
  await page.getByLabel('Backup Password', { exact: true }).fill('test-password-123');
  await page.getByLabel('Confirm Backup Password', { exact: true }).fill('test-password-123');
  await page.getByRole('button', { name: /sign up/i }).click();
  await expect(page.getByText(/Account Created/i)).toBeVisible({ timeout: 15_000 });

  // Fresh context simulates a new device — no IDB clearing needed, avoids
  // the webkit race where deleteDatabase hasn't completed before the new
  // page's DeviceProvider reads stale data and redirects away from /signup.
  const { baseURL } = test.info().project.use;
  const freshCtx = await browser.newContext({ baseURL });
  const freshPage = await freshCtx.newPage();

  try {
    await freshPage.goto('/signup');
    await expect(freshPage.getByLabel(/username/i)).toBeVisible();

    await freshPage.getByLabel(/username/i).fill(username);
    await freshPage.getByLabel('Backup Password', { exact: true }).fill('test-password-123');
    await freshPage
      .getByLabel('Confirm Backup Password', { exact: true })
      .fill('test-password-123');
    await freshPage.getByRole('button', { name: /sign up/i }).click();

    // Should show an error (duplicate username)
    await expect(freshPage.getByText(/Signup failed/i)).toBeVisible({ timeout: 15_000 });
  } finally {
    await freshCtx.close();
  }
});
