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
  await page.getByRole('button', { name: /sign up/i }).click();

  // Wait for success — timeout accounts for WASM loading + key generation + API call
  await expect(page.getByText(/Account Created/i)).toBeVisible({ timeout: 15_000 });

  // Verify M2 contract: account ID, root KID, and device KID are all displayed
  await expect(page.getByText(/Account ID:/i)).toBeVisible();
  await expect(page.getByText(/Root Key ID:/i)).toBeVisible();
  await expect(page.getByText(/Device Key ID:/i)).toBeVisible();

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

test('signup shows error for duplicate username @smoke', async ({ page }) => {
  const username = `dup-user-${String(Date.now())}`;

  await page.goto('/signup');
  await expect(page.getByLabel(/username/i)).toBeVisible();

  // First signup should succeed
  await page.getByLabel(/username/i).fill(username);
  await page.getByRole('button', { name: /sign up/i }).click();
  await expect(page.getByText(/Account Created/i)).toBeVisible({ timeout: 15_000 });

  // Navigate back to signup for a second attempt with the same username
  await page.goto('/signup');
  await expect(page.getByLabel(/username/i)).toBeVisible();

  await page.getByLabel(/username/i).fill(username);
  await page.getByRole('button', { name: /sign up/i }).click();

  // Should show an error (duplicate username)
  await expect(page.getByText(/Signup failed/i)).toBeVisible({ timeout: 15_000 });
});
