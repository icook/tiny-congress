/**
 * Identity E2E integration tests - full flow testing with backend
 *
 * These tests require the full backend stack to be running.
 * In CI, backend must be available - tests will fail if not.
 * Locally, set SKIP_INTEGRATION_TESTS=1 to skip these tests.
 */

/* eslint-disable playwright/no-skipped-test */

import { expect, test } from './fixtures';

const API_URL = process.env.PLAYWRIGHT_API_URL ?? 'http://127.0.0.1:8080';
const IS_CI = process.env.CI === 'true';
const SKIP_LOCALLY = process.env.SKIP_INTEGRATION_TESTS === '1';

// Check backend availability - fail in CI, skip locally if requested
async function checkBackendOrSkip(request: Parameters<Parameters<typeof test>[1]>[0]['request']) {
  if (SKIP_LOCALLY && !IS_CI) {
    test.skip(true, 'Skipping integration tests locally (SKIP_INTEGRATION_TESTS=1)');
    return;
  }

  try {
    const response = await request.get(`${API_URL}/health`);
    if (!response.ok()) {
      if (IS_CI) {
        throw new Error(`Backend health check failed in CI: ${response.status()}`);
      }
      test.skip(true, 'Backend not available locally');
    }
  } catch (error) {
    if (IS_CI) {
      throw new Error(`Backend not reachable in CI: ${error}`);
    }
    test.skip(true, 'Backend not available locally');
  }
}

test.describe('Signup flow @integration', () => {
  test('completes signup with valid credentials', async ({ page, request }) => {
    await checkBackendOrSkip(request);

    await page.goto('/signup');

    // Generate unique username for this test run
    const uniqueUsername = `testuser_${Date.now()}`;

    // Fill in signup form
    await page.getByLabel(/username/i).fill(uniqueUsername);
    await page.getByLabel(/device name/i).fill('E2E Test Device');

    // Submit form
    await page.getByRole('button', { name: /sign up/i }).click();

    // Wait for navigation to dashboard (success case)
    await expect(page).toHaveURL(/dashboard/, { timeout: 10000 });
  });

  test('shows error for duplicate username', async ({ page, request }) => {
    await checkBackendOrSkip(request);

    // First, create a user directly via API if possible, or use a known existing user
    const existingUsername = 'duplicate_test_user';

    await page.goto('/signup');

    // Try to sign up with existing username
    await page.getByLabel(/username/i).fill(existingUsername);
    await page.getByLabel(/device name/i).fill('E2E Test Device');
    await page.getByRole('button', { name: /sign up/i }).click();

    // Should show error message (either stays on page with error or shows notification)
    // The exact error handling depends on implementation
    await expect(page.getByText(/already|exists|taken/i)).toBeVisible({ timeout: 5000 });
  });
});

test.describe('Login flow @integration', () => {
  // These tests require a pre-existing account
  // In CI, this would be set up as part of test fixtures

  test('shows device key warning when no key exists', async ({ page, request }) => {
    await checkBackendOrSkip(request);

    await page.goto('/login');

    // Enter fake account/device IDs
    await page.getByLabel(/account id/i).fill('00000000-0000-0000-0000-000000000000');
    await page.getByLabel(/device id/i).fill('00000000-0000-0000-0000-000000000001');

    // Should show warning about missing device key
    await expect(page.getByText(/no device key found/i)).toBeVisible({ timeout: 3000 });
  });
});

test.describe('Navigation between auth pages @integration', () => {
  test('can navigate from home to signup', async ({ page, request }) => {
    await checkBackendOrSkip(request);

    await page.goto('/');
    await page.goto('/signup');

    await expect(page.getByRole('heading', { name: /create account/i })).toBeVisible();
  });

  test('can navigate from signup to login', async ({ page, request }) => {
    await checkBackendOrSkip(request);

    await page.goto('/signup');
    await page.goto('/login');

    await expect(page.getByRole('heading', { name: /login/i })).toBeVisible();
  });
});
