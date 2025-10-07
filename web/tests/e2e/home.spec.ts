import { expect, test } from './fixtures';

test('home page renders welcome headline', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('heading', { name: /welcome to/i })).toBeVisible();
});
