import { expect, test } from './fixtures';

test('signup flow creates account @smoke', async ({ page }) => {
  // Generate unique username to avoid conflicts
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

  // Wait for success (account details appear)
  // Timeout accounts for WASM loading + key generation + API call
  await expect(page.getByText(/Account Created/i)).toBeVisible({ timeout: 15000 });
  await expect(page.getByText(/Account ID:/i)).toBeVisible();
  await expect(page.getByText(/Root Key ID:/i)).toBeVisible();

  // Screenshot 2: Success state
  await test.info().attach('signup-success', {
    body: await page.screenshot(),
    contentType: 'image/png',
  });
});
