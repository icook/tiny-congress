import { existsSync, mkdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { expect, test } from './fixtures';
import { signupUser } from './helpers';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const SCREENSHOTS_DIR = path.join(process.cwd(), 'screenshots');
const DESKTOP = { width: 1280, height: 720 };
const MOBILE = { width: 390, height: 844 };

// Visual regression baselines live next to the spec file.
// When baselines are committed, capture() asserts pixel-level consistency.
// When missing (first run or baselines not yet generated), only gallery
// screenshots are saved — no regression failure.
// CI passes GENERATE_BASELINES=true with --update-snapshots to create initial baselines.
const BASELINES_DIR = path.join(__dirname, 'screenshots.spec.ts-snapshots');
const generateBaselines = process.env.GENERATE_BASELINES === 'true';
const hasBaselines = existsSync(BASELINES_DIR) || generateBaselines;

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

  // Save for gallery (always)
  await page.screenshot({
    path: path.join(SCREENSHOTS_DIR, `${name}.png`),
    fullPage: true,
  });

  // Visual regression assertion (only when baselines are committed)
  if (hasBaselines) {
    await expect(page).toHaveScreenshot(`${name}.png`, { fullPage: true });
  }
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
    await expect(page.getByText(/login failed/i)).toBeVisible({ timeout: 10_000 });
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

  // --- Authenticated pages ---

  test('signup success', async ({ page }) => {
    await signupUser(page);
    await expect(page.getByText(/welcome/i)).toBeVisible();
    await capture(page, 'signup-success-desktop-dark', { viewport: DESKTOP, colorScheme: 'dark' });
  });

  test('rooms list', async ({ page }) => {
    await signupUser(page);
    await page.goto('/rooms');
    await expect(page.getByRole('heading', { name: /rooms/i })).toBeVisible();
    await capture(page, 'rooms-desktop-dark', { viewport: DESKTOP, colorScheme: 'dark' });
    await capture(page, 'rooms-mobile-dark', { viewport: MOBILE, colorScheme: 'dark' });
  });

  test('poll page', async ({ page }) => {
    await signupUser(page);
    await page.goto('/rooms');

    const pollLink = page.locator('a[href*="/polls/"]').first();
    const pollExists = await pollLink.isVisible({ timeout: 5_000 }).catch(() => false);
    // eslint-disable-next-line playwright/no-skipped-test -- runtime skip if no seeded polls
    test.skip(!pollExists, 'No polls seeded in this environment');

    await pollLink.click();
    await expect(page.locator('h1, h2, h3').first()).toBeVisible();
    await capture(page, 'poll-desktop-dark', { viewport: DESKTOP, colorScheme: 'dark' });
    await capture(page, 'poll-mobile-dark', { viewport: MOBILE, colorScheme: 'dark' });
  });

  test('settings page', async ({ page }) => {
    await signupUser(page);

    // Pin device timestamps so date column widths are deterministic.
    // Without this, single-digit days ("Apr 8") vs double-digit ("Apr 24")
    // cause different text wrapping in the narrow mobile table columns.
    const fixedTimestamp = '2025-01-15T12:00:00+00:00';
    await page.route('**/auth/devices', async (route) => {
      const response = await route.fetch();
      const json = await response.json();
      for (const device of json.devices) {
        device.created_at = fixedTimestamp;
        device.last_used_at = fixedTimestamp;
      }
      await route.fulfill({ response, json });
    });

    await page.goto('/settings');
    await expect(page.getByRole('heading', { name: /devices/i })).toBeVisible({ timeout: 10_000 });
    await capture(page, 'settings-desktop-dark', { viewport: DESKTOP, colorScheme: 'dark' });
    await capture(page, 'settings-mobile-dark', { viewport: MOBILE, colorScheme: 'dark' });
  });
});
