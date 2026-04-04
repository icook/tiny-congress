import { expect, test } from './fixtures';
import { seedRoomWithPoll, signupUser } from './helpers';

test.describe('voting gates', () => {
  let roomId: string;
  let pollId: string;

  test.beforeAll(async ({ browser }) => {
    const baseURL = process.env.PLAYWRIGHT_BASE_URL ?? 'http://127.0.0.1:4173';
    const ctx = await browser.newContext({ baseURL });
    const page = await ctx.newPage();

    await signupUser(page, `seed-owner-${String(Date.now())}`);
    const data = await seedRoomWithPoll(page);
    roomId = data.roomId;
    pollId = data.pollId;

    await ctx.close();
  });

  test('unverified user sees eligibility gate on poll page @smoke', async ({ page }) => {
    await signupUser(page);

    await page.goto(`/rooms/${roomId}/polls/${pollId}`);
    await expect(
      page.getByRole('heading', { name: /should we increase park funding/i })
    ).toBeVisible({ timeout: 10_000 });

    // Non-owner user without endorsement sees eligibility gate
    await expect(page.getByText(/not eligible to vote/i)).toBeVisible({ timeout: 10_000 });

    // Sliders should be disabled (Mantine sets aria-disabled on the thumb div)
    await expect(page.locator('[role="slider"]').first()).toHaveAttribute('aria-disabled', 'true');

    await test.info().attach('eligibility-gate', {
      body: await page.screenshot(),
      contentType: 'image/png',
    });
  });

  test('guest user sees login prompt on poll page @smoke', async ({ page }) => {
    await page.goto(`/rooms/${roomId}/polls/${pollId}`);

    // Should see login/signup prompt
    await expect(page.getByText(/sign up/i)).toBeVisible({ timeout: 10_000 });

    await test.info().attach('guest-poll-gate', {
      body: await page.screenshot(),
      contentType: 'image/png',
    });
  });
});
