import { expect, test } from './fixtures';

test('rooms page loads and shows content', async ({ page }) => {
  await page.goto('/rooms');

  // Page title should render
  await expect(page.getByRole('heading', { name: /Rooms/i })).toBeVisible();

  // Either shows rooms or the empty state message (depending on DB state)
  const emptyState = page.getByText(/No rooms are currently open/i);
  const roomHeading = page.getByRole('heading', { level: 4 }).first();

  await expect(emptyState.or(roomHeading)).toBeVisible({ timeout: 10_000 });
});

test('rooms page is accessible from navbar', async ({ page }) => {
  await page.goto('/');

  // On mobile viewports the navbar is behind a burger menu
  const burger = page.getByRole('button', { name: /toggle navigation/i });
  // eslint-disable-next-line playwright/no-conditional-in-test
  if (await burger.isVisible()) {
    await burger.click();
  }

  // Navbar should have a Rooms link (use exact match to avoid the "Browse Rooms" CTA)
  const roomsLink = page.getByRole('link', { name: /^Rooms$/i });
  await expect(roomsLink).toBeVisible();

  // Click it
  await roomsLink.click();

  // Should navigate to rooms page
  await expect(page).toHaveURL(/\/rooms/);
  await expect(page.getByRole('heading', { name: /Rooms/i })).toBeVisible();
});

test('non-existent poll shows error', async ({ page }) => {
  // Navigate to a poll with fake UUIDs
  await page.goto(
    '/rooms/00000000-0000-0000-0000-000000000000/polls/00000000-0000-0000-0000-000000000001'
  );

  // Should show an error alert (poll not found from API)
  await expect(page.getByText(/Poll not found/i)).toBeVisible({ timeout: 10_000 });
});
