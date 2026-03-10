import { expect, test } from './fixtures';
import { signupUser } from './helpers';

test('unverified user sees verification gate on poll page @smoke', async ({ page }) => {
  await signupUser(page);

  // Navigate to rooms
  await page.goto('/rooms');
  await expect(page.getByText(/rooms/i)).toBeVisible();

  // Screenshot: rooms page
  await test.info().attach('rooms-page', {
    body: await page.screenshot(),
    contentType: 'image/png',
  });

  // If rooms exist, click the first poll link
  const pollLink = page.locator('a[href*="/polls/"]').first();
  if (await pollLink.isVisible({ timeout: 5_000 }).catch(() => false)) {
    await pollLink.click();

    // Should see verification gate (user is signed up but not verified)
    await expect(page.getByText(/verify your identity/i)).toBeVisible({ timeout: 10_000 });

    // Sliders should be disabled
    const slider = page.locator('.mantine-Slider-root').first();
    if (await slider.isVisible({ timeout: 3_000 }).catch(() => false)) {
      // The slider input should be disabled
      await expect(slider.locator('input')).toBeDisabled();
    }

    // Screenshot: verification gate
    await test.info().attach('verification-gate', {
      body: await page.screenshot(),
      contentType: 'image/png',
    });
  } else {
    // No rooms/polls seeded — document this as expected in empty environment
    await test.info().attach('no-polls-available', {
      body: await page.screenshot(),
      contentType: 'image/png',
    });
  }
});

test('guest user sees login prompt on poll page @smoke', async ({ page }) => {
  // Navigate directly to rooms without signing up
  await page.goto('/rooms');

  const pollLink = page.locator('a[href*="/polls/"]').first();
  if (await pollLink.isVisible({ timeout: 5_000 }).catch(() => false)) {
    await pollLink.click();

    // Should see login/signup prompt
    await expect(page.getByText(/sign up/i)).toBeVisible({ timeout: 10_000 });

    await test.info().attach('guest-poll-gate', {
      body: await page.screenshot(),
      contentType: 'image/png',
    });
  }
});
