import { expect, test } from './fixtures';

test('rooms page loads and shows empty state', async ({ page }) => {
  await page.goto('/rooms');

  // Page title should render
  await expect(page.getByRole('heading', { name: /Rooms/i })).toBeVisible();

  // Subtitle should render
  await expect(page.getByText(/Browse open rooms and participate in polls/i)).toBeVisible();

  // Either shows rooms or the empty state message
  const content = page.getByText(/No rooms are currently open/i);
  const roomCard = page.locator('[data-testid="room-card"]').first();

  // One of these should be visible (depending on DB state)
  await expect(content.or(roomCard)).toBeVisible({ timeout: 10_000 });
});

test('rooms page is accessible from navbar', async ({ page }) => {
  await page.goto('/');

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
  await expect(page.getByText(/Failed to load poll/i)).toBeVisible({ timeout: 10_000 });
});
