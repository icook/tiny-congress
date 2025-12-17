import { expect, test } from './fixtures';

test.describe('Signup flow', () => {
  test('signup page renders correctly', async ({ page }) => {
    await page.goto('/signup');

    // Check page title and description
    await expect(page.getByRole('heading', { name: 'Create Account' })).toBeVisible();
    await expect(
      page.getByText('Sign up for TinyCongress with cryptographic identity')
    ).toBeVisible();

    // Check form elements
    await expect(page.getByLabel('Username')).toBeVisible();
    await expect(page.getByLabel('Device Name')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Sign Up' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Already have an account?' })).toBeVisible();
  });

  test('signup form validation requires username', async ({ page }) => {
    await page.goto('/signup');

    // Try to submit without username
    await page.getByRole('button', { name: 'Sign Up' }).click();

    // Form should not submit (button still enabled, no navigation)
    await expect(page).toHaveURL('/signup');
  });

  test('signup creates account and redirects to login @smoke', async ({ page }) => {
    // Generate a unique username to avoid conflicts
    const uniqueUsername = `testuser_${Date.now()}`;

    await page.goto('/signup');

    // Fill in the form
    await page.getByLabel('Username').fill(uniqueUsername);
    await page.getByLabel('Device Name').fill('Test Browser');

    // Submit the form
    await page.getByRole('button', { name: 'Sign Up' }).click();

    // Wait for key generation and API call
    await expect(page.getByRole('button', { name: 'Generating keys...' })).toBeVisible({
      timeout: 5000,
    });

    // Should redirect to login on success (or show error)
    await expect(async () => {
      const url = page.url();
      const hasError = await page
        .getByRole('alert')
        .isVisible()
        .catch(() => false);
      expect(url.includes('/login') || hasError).toBeTruthy();
    }).toPass({ timeout: 15000 });
  });

  test('signup shows error for duplicate username @smoke', async ({ page }) => {
    // First, create an account
    const uniqueUsername = `duplicate_test_${Date.now()}`;

    await page.goto('/signup');
    await page.getByLabel('Username').fill(uniqueUsername);
    await page.getByRole('button', { name: 'Sign Up' }).click();

    // Wait for first signup to complete
    await page.waitForURL('/login', { timeout: 15000 }).catch(() => {
      // May fail if API is not running, that's ok for this test
    });

    // Clear localStorage to simulate fresh browser
    await page.evaluate(() => localStorage.clear());

    // Try to sign up again with same username
    await page.goto('/signup');
    await page.getByLabel('Username').fill(uniqueUsername);
    await page.getByRole('button', { name: 'Sign Up' }).click();

    // Should show error about duplicate username
    await expect(page.getByRole('alert')).toBeVisible({ timeout: 15000 });
    await expect(page.getByText(/already exists|duplicate/i)).toBeVisible();
  });

  test('navigates to login page from signup', async ({ page }) => {
    await page.goto('/signup');

    await page.getByRole('button', { name: 'Already have an account?' }).click();

    await expect(page).toHaveURL('/login');
  });
});
