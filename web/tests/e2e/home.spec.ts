import { expect, test } from '@playwright/test';

test('home page renders welcome headline', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('heading', { name: /welcome to/i })).toBeVisible();
});
