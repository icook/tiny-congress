import crypto from 'node:crypto';
import fs from 'node:fs/promises';
import path from 'node:path';
import { test as base, expect } from '@playwright/test';

const truthy = (value: string | undefined) =>
  (value ?? '').toLowerCase() === 'true' || value === '1';
const shouldCollectCoverage = truthy(process.env.PLAYWRIGHT_COVERAGE) || truthy(process.env.CI);

// Extended test fixture with V8 coverage collection
export const test = base.extend<{ coveragePage: void }>({
  coveragePage: [
    async ({ page }, use, testInfo) => {
      if (shouldCollectCoverage) {
        // Start V8 JavaScript coverage before test
        await page.coverage.startJSCoverage({ resetOnNavigation: false });
      }

      // Run the test
      await use();

      if (shouldCollectCoverage) {
        // Stop coverage and get results
        const coverage = await page.coverage.stopJSCoverage();

        if (coverage.length > 0) {
          const coverageDir = path.join(process.cwd(), '.nyc_output');
          await fs.mkdir(coverageDir, { recursive: true });

          // Generate unique filename
          const hash = crypto.randomBytes(8).toString('hex');
          const safeId = testInfo.testId.replace(/[^a-z0-9_-]/gi, '_');
          const filePath = path.join(coverageDir, `v8-${safeId}-${hash}.json`);

          // Write V8 coverage format that c8 can read
          await fs.writeFile(filePath, JSON.stringify({ result: coverage }));
        }
      }
    },
    { auto: true }, // Automatically use this fixture for all tests
  ],
});

export { expect };
