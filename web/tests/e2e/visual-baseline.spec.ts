/**
 * Page load smoke tests — verifies that every major page renders without
 * JavaScript errors and shows expected content.
 *
 * Visual screenshot comparison baselines can be added via:
 *   npx playwright test visual-baseline.spec.ts --update-snapshots
 * once a stable CI environment with WASM artifacts is available.
 */
import { expect, test } from './fixtures';
import { seedRoomWithPoll, signupUser } from './helpers';

test.describe('page load baselines', () => {
  test('home page renders @smoke', async ({ page }) => {
    await page.goto('/');
    await expect(page.getByRole('heading', { name: /TinyCongress/i })).toBeVisible();
  });

  test('rooms page loads @smoke', async ({ page }) => {
    await page.goto('/rooms');
    await page.waitForLoadState('load');
    // Either rooms list or empty state renders without error
    await expect(page.locator('body')).not.toContainText('Something went wrong');
  });

  test('about page loads @smoke', async ({ page }) => {
    await page.goto('/about');
    await page.waitForLoadState('load');
    await expect(page.getByRole('heading', { name: /about/i }).first()).toBeVisible();
  });

  test('signup page renders form @smoke', async ({ page }) => {
    await page.goto('/signup');
    await expect(page.getByLabel(/username/i)).toBeVisible();
    await expect(page.getByRole('button', { name: /sign up/i })).toBeVisible();
  });

  test('login page renders form @smoke', async ({ page }) => {
    await page.goto('/login');
    await expect(page.getByLabel(/username/i)).toBeVisible();
    await expect(page.getByRole('button', { name: /log in/i })).toBeVisible();
  });

  test('settings page redirects when unauthenticated @smoke', async ({ page }) => {
    await page.goto('/settings');
    // Should redirect to login
    await expect(page.getByLabel(/username/i)).toBeVisible({ timeout: 5_000 });
    expect(page.url()).toContain('/login');
  });

  test('settings page loads when authenticated @smoke', async ({ page }) => {
    await signupUser(page);
    await page.goto('/settings');
    await expect(page.getByRole('heading', { name: /devices/i })).toBeVisible({
      timeout: 10_000,
    });
  });
});

test.describe('poll flow baselines', () => {
  test('room detail with active poll @smoke', async ({ page }) => {
    await signupUser(page);
    const { roomId } = await seedRoomWithPoll(page);

    await page.goto(`/rooms/${roomId}`);
    await expect(page.getByText(/should we increase park funding/i)).toBeVisible({
      timeout: 10_000,
    });

    await test.info().attach('room-detail-with-poll', {
      body: await page.screenshot(),
      contentType: 'image/png',
    });
  });

  test('poll page vote and results @smoke', async ({ page }) => {
    await signupUser(page);
    const { roomId, pollId } = await seedRoomWithPoll(page);

    await page.goto(`/rooms/${roomId}/polls/${pollId}`);
    await expect(
      page.getByRole('heading', { name: /should we increase park funding/i })
    ).toBeVisible({ timeout: 10_000 });

    // Pre-vote screenshot (owner can vote)
    await test.info().attach('poll-pre-vote', {
      body: await page.screenshot(),
      contentType: 'image/png',
    });

    // Submit vote
    await page.getByRole('button', { name: /submit vote/i }).click();
    await expect(page.getByText(/thanks for voting/i)).toBeVisible({ timeout: 10_000 });

    // Post-vote screenshot with results
    await test.info().attach('poll-post-vote', {
      body: await page.screenshot(),
      contentType: 'image/png',
    });
  });
});
