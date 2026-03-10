import { expect, type Page } from './fixtures';

/**
 * Sign up a new user and wait for the success screen.
 * Returns the username used.
 */
export async function signupUser(
  page: Page,
  username?: string,
  password = 'test-password-123'
): Promise<string> {
  const name = username ?? `test-user-${String(Date.now())}`;
  await page.goto('/signup');
  await expect(page.getByLabel(/username/i)).toBeVisible();
  await page.getByLabel(/username/i).fill(name);
  await page.getByLabel(/backup password/i).fill(password);
  await page.getByRole('button', { name: /sign up/i }).click();
  await expect(page.getByText(/Account Created/i)).toBeVisible({ timeout: 15_000 });
  return name;
}
