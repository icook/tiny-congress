import { existsSync, mkdirSync } from 'node:fs';
import path from 'node:path';
import { expect, test } from './fixtures';

const SCREENSHOTS_DIR = path.join(process.cwd(), 'screenshots');
const DESKTOP = { width: 1280, height: 720 };
const MOBILE = { width: 390, height: 844 };

if (!existsSync(SCREENSHOTS_DIR)) {
  mkdirSync(SCREENSHOTS_DIR, { recursive: true });
}

async function capture(
  page: import('@playwright/test').Page,
  name: string,
  options?: { viewport?: { width: number; height: number }; colorScheme?: 'dark' | 'light' }
): Promise<void> {
  if (options?.viewport) {
    await page.setViewportSize(options.viewport);
  }
  if (options?.colorScheme) {
    await page.emulateMedia({ colorScheme: options.colorScheme });
  }
  await page.waitForLoadState('load');
  await page.screenshot({
    path: path.join(SCREENSHOTS_DIR, `${name}.png`),
    fullPage: true,
  });
}

test.describe.serial('screenshot gallery @screenshots', () => {
  test('landing page', async ({ page }) => {
    await page.goto('/');
    await expect(page.getByRole('heading', { name: /TinyCongress/i })).toBeVisible();
    await capture(page, 'landing-desktop-dark', { viewport: DESKTOP, colorScheme: 'dark' });
    await capture(page, 'landing-desktop-light', { viewport: DESKTOP, colorScheme: 'light' });
    await capture(page, 'landing-mobile-dark', { viewport: MOBILE, colorScheme: 'dark' });
    await capture(page, 'landing-mobile-light', { viewport: MOBILE, colorScheme: 'light' });
  });

  test('about page', async ({ page }) => {
    await page.goto('/about');
    await expect(page.getByRole('heading', { name: /about/i }).first()).toBeVisible();
    await capture(page, 'about-desktop-dark', { viewport: DESKTOP, colorScheme: 'dark' });
    await capture(page, 'about-desktop-light', { viewport: DESKTOP, colorScheme: 'light' });
  });

  test('login page', async ({ page }) => {
    await page.goto('/login');
    await expect(page.getByRole('button', { name: /log in/i })).toBeVisible();
    await capture(page, 'login-desktop-dark', { viewport: DESKTOP, colorScheme: 'dark' });
  });

  test('login error', async ({ page }) => {
    await page.goto('/login');
    await page.getByLabel(/username/i).fill('nonexistent-user');
    await page.getByLabel(/password/i).fill('wrong-password');
    await page.getByRole('button', { name: /log in/i }).click();
    await expect(page.getByText(/not found|invalid|error/i)).toBeVisible({ timeout: 10_000 });
    await capture(page, 'login-error-desktop-dark', { viewport: DESKTOP, colorScheme: 'dark' });
  });

  test('signup page', async ({ page }) => {
    await page.goto('/signup');
    await expect(page.getByLabel(/username/i)).toBeVisible();
    await capture(page, 'signup-desktop-dark', { viewport: DESKTOP, colorScheme: 'dark' });
  });

  test('404 page', async ({ page }) => {
    await page.goto('/this-page-does-not-exist');
    await expect(page.getByRole('main')).toBeVisible();
    await capture(page, '404-desktop-dark', { viewport: DESKTOP, colorScheme: 'dark' });
  });
});
