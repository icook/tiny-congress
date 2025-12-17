import { defineConfig, devices } from '@playwright/test';

const truthy = (value: string | undefined) =>
  (value ?? '').toLowerCase() === 'true' || value === '1';

const shouldStartWebServer = !truthy(process.env.PLAYWRIGHT_SKIP_WEB_SERVER) && !process.env.CI;

export default defineConfig({
  testDir: './tests/e2e',
  globalTeardown: './tests/e2e/global-teardown.ts',
  timeout: 30_000,
  fullyParallel: true,
  retries: process.env.CI ? 1 : 0,
  reporter: [
    ['list'],
    [
      'junit',
      {
        outputFile: 'reports/playwright.xml',
        embedAnnotationsAsProperties: true,
      },
    ],
    ['html', { outputFolder: 'playwright-report', open: 'never' }],
  ],
  use: {
    baseURL: process.env.PLAYWRIGHT_BASE_URL ?? 'http://127.0.0.1:4173',
    headless: true,
    viewport: { width: 1280, height: 720 },
    ignoreHTTPSErrors: true,
    trace: 'retain-on-failure',
    video: 'retain-on-failure',
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
  webServer: shouldStartWebServer
    ? {
        command: 'yarn build && yarn preview --host 0.0.0.0 --port 4173 --strictPort',
        url: 'http://127.0.0.1:4173',
        reuseExistingServer: true,
        timeout: 120_000,
      }
    : undefined,
});
