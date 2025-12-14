/**
 * Identity E2E smoke tests - verify pages render correctly without backend
 */

import { expect, test } from './fixtures';

test.describe('Signup page', () => {
  test('renders signup form', async ({ page }) => {
    await page.goto('/signup');

    await expect(page.getByRole('heading', { name: /create account/i })).toBeVisible();
    await expect(page.getByLabel(/username/i)).toBeVisible();
    await expect(page.getByLabel(/device name/i)).toBeVisible();
    await expect(page.getByRole('button', { name: /sign up/i })).toBeVisible();
  });

  test('validates username is required', async ({ page }) => {
    await page.goto('/signup');

    // Try submitting with empty form
    await page.getByRole('button', { name: /sign up/i }).click();

    await expect(page.getByText(/username is required/i)).toBeVisible();
  });

  test('validates username format', async ({ page }) => {
    await page.goto('/signup');

    // Enter invalid username with spaces
    await page.getByLabel(/username/i).fill('has spaces');
    await page.getByLabel(/device name/i).fill('Test Device');
    await page.getByRole('button', { name: /sign up/i }).click();

    await expect(
      page.getByText(/username can only contain letters, numbers, hyphens, and underscores/i)
    ).toBeVisible();
  });

  test('validates username minimum length', async ({ page }) => {
    await page.goto('/signup');

    // Enter short username
    await page.getByLabel(/username/i).fill('ab');
    await page.getByLabel(/device name/i).fill('Test Device');
    await page.getByRole('button', { name: /sign up/i }).click();

    await expect(page.getByText(/username must be at least 3 characters/i)).toBeVisible();
  });

  test('validates device name is required', async ({ page }) => {
    await page.goto('/signup');

    await page.getByLabel(/username/i).fill('testuser');
    await page.getByRole('button', { name: /sign up/i }).click();

    await expect(page.getByText(/device name is required/i)).toBeVisible();
  });
});

test.describe('Login page', () => {
  test('renders login form', async ({ page }) => {
    await page.goto('/login');

    await expect(page.getByRole('heading', { name: /login/i })).toBeVisible();
    await expect(page.getByLabel(/account id/i)).toBeVisible();
    await expect(page.getByLabel(/device id/i)).toBeVisible();
    await expect(page.getByRole('button', { name: /request challenge/i })).toBeVisible();
  });

  test('validates account id is required', async ({ page }) => {
    await page.goto('/login');

    // Enter only device id
    await page.getByLabel(/device id/i).fill('some-device-id');
    await page.getByRole('button', { name: /request challenge/i }).click();

    await expect(page.getByText(/account id is required/i)).toBeVisible();
  });

  test('validates device id is required', async ({ page }) => {
    await page.goto('/login');

    // Enter only account id
    await page.getByLabel(/account id/i).fill('some-account-id');
    await page.getByRole('button', { name: /request challenge/i }).click();

    await expect(page.getByText(/device id is required/i)).toBeVisible();
  });
});

test.describe('Devices page', () => {
  test('renders devices page', async ({ page }) => {
    await page.goto('/devices');

    await expect(page.getByRole('heading', { name: /device management/i })).toBeVisible();
  });
});

test.describe('Profile page', () => {
  test('renders profile page', async ({ page }) => {
    await page.goto('/account');

    await expect(page.getByRole('heading', { name: /my profile/i })).toBeVisible();
  });
});

test.describe('Endorsements page', () => {
  test('renders endorsements page', async ({ page }) => {
    await page.goto('/endorsements');

    await expect(page.getByRole('heading', { name: /endorsements/i })).toBeVisible();
  });
});

test.describe('Recovery page', () => {
  test('renders recovery page', async ({ page }) => {
    await page.goto('/recovery');

    await expect(page.getByRole('heading', { name: /recovery setup/i })).toBeVisible();
  });
});
