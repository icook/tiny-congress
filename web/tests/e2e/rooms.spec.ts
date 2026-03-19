import { expect, test } from './fixtures';

test('rooms page loads and shows content', async ({ page }) => {
  await page.goto('/rooms');

  // Page title should render
  await expect(page.getByRole('heading', { name: /Rooms/i })).toBeVisible();

  // Either shows rooms or the empty state message (depending on DB state)
  const emptyState = page.getByText(/No rooms are (currently open|open right now)/i);
  const roomHeading = page.getByRole('heading', { level: 4 }).first();

  await expect(emptyState.or(roomHeading)).toBeVisible({ timeout: 10_000 });
});

test('rooms page is accessible from navbar', async ({ page }) => {
  await page.goto('/');

  // On mobile viewports the navbar is behind a burger menu.
  // Try to open it; if it doesn't exist (desktop) the click is skipped.
  const burger = page.locator('.mantine-Burger-root');
  await burger.click({ timeout: 2_000 }).catch(() => {
    /* burger not present on desktop — expected */
  });

  // "Rooms" is an accordion NavLink — click to expand, then click "All Rooms" to navigate
  const roomsAccordion = page.locator('.mantine-NavLink-root', { hasText: /^Rooms$/ });
  await expect(roomsAccordion).toBeVisible({ timeout: 5_000 });
  await roomsAccordion.click();

  const allRoomsLink = page.getByRole('link', { name: /All Rooms/i });
  await expect(allRoomsLink).toBeVisible({ timeout: 5_000 });
  await allRoomsLink.click();

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
