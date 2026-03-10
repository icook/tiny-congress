import { expect, test } from './fixtures';
import { signupUser } from './helpers';

test('guest nav links all resolve @smoke', async ({ page }) => {
  await page.goto('/');

  // Home renders
  await expect(page.getByText(/TinyCongress/i)).toBeVisible();

  // Rooms is accessible
  await page.goto('/rooms');
  await page.waitForLoadState('load');

  // About is accessible
  await page.goto('/about');
  await expect(page.getByText(/about/i)).toBeVisible();

  // Settings redirects to login
  await page.goto('/settings');
  await expect(page.getByLabel(/username/i)).toBeVisible({ timeout: 5_000 });
  expect(page.url()).toContain('/login');
});

test('authenticated nav links all resolve @smoke', async ({ page }) => {
  await signupUser(page);

  // Settings is accessible after signup
  await page.goto('/settings');
  await expect(page.getByText(/devices/i)).toBeVisible({ timeout: 10_000 });

  // Login/signup redirect to rooms when authenticated
  await page.goto('/signup');
  expect(page.url()).toContain('/rooms');

  await page.goto('/login');
  expect(page.url()).toContain('/rooms');
});
