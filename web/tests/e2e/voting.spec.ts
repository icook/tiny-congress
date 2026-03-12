import { expect, test } from './fixtures';
import { signupUser } from './helpers';

test('unverified user sees verification gate on poll page @smoke', async ({ page }) => {
  await signupUser(page);

  // Navigate to rooms
  await page.goto('/rooms');
  await expect(page.getByRole('heading', { name: /rooms/i })).toBeVisible();

  // Screenshot: rooms page
  await test.info().attach('rooms-page', {
    body: await page.screenshot(),
    contentType: 'image/png',
  });

  // Skip if no polls are seeded in this environment
  const pollLink = page.locator('a[href*="/polls/"]').first();
  const pollExists = await pollLink.isVisible({ timeout: 5_000 }).catch(() => false);
  test.skip(!pollExists, 'No polls seeded in this environment');

  await pollLink.click();

  // Should see verification gate (user is signed up but not verified)
  await expect(page.getByText(/verify your identity/i)).toBeVisible({ timeout: 10_000 });

  // Sliders should be disabled
  await expect(page.locator('.mantine-Slider-root input').first()).toBeDisabled();

  // Screenshot: verification gate
  await test.info().attach('verification-gate', {
    body: await page.screenshot(),
    contentType: 'image/png',
  });
});

test('guest user sees login prompt on poll page @smoke', async ({ page }) => {
  // Navigate directly to rooms without signing up
  await page.goto('/rooms');

  // Skip if no polls are seeded in this environment
  const pollLink = page.locator('a[href*="/polls/"]').first();
  const pollExists = await pollLink.isVisible({ timeout: 5_000 }).catch(() => false);
  test.skip(!pollExists, 'No polls seeded in this environment');

  await pollLink.click();

  // Should see login/signup prompt
  await expect(page.getByText(/sign up/i)).toBeVisible({ timeout: 10_000 });

  await test.info().attach('guest-poll-gate', {
    body: await page.screenshot(),
    contentType: 'image/png',
  });
});
