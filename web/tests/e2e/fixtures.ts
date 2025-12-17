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
        // eslint-disable-next-line no-console
        console.log(`[V8 Coverage] Starting coverage for: ${testInfo.title}`);
        await page.coverage.startJSCoverage({ resetOnNavigation: false });
      }

      // Run the test
      await use();

      if (shouldCollectCoverage) {
        const coverage = await page.coverage.stopJSCoverage();
        // eslint-disable-next-line no-console
        console.log(`[V8 Coverage] Collected ${coverage.length} scripts for: ${testInfo.title}`);

        if (coverage.length > 0) {
          const coverageDir = path.join(process.cwd(), '.nyc_output');
          await fs.mkdir(coverageDir, { recursive: true });

          const hash = crypto.randomBytes(8).toString('hex');
          const safeId = testInfo.testId.replace(/[^a-z0-9_-]/gi, '_');
          const filePath = path.join(coverageDir, `v8-${safeId}-${hash}.json`);

          // Write V8 coverage in the format c8 expects
          await fs.writeFile(filePath, JSON.stringify({ result: coverage }));
          // eslint-disable-next-line no-console
          console.log(`[V8 Coverage] Wrote: ${filePath}`);
        }
      }
    },
    { auto: true },
  ],
});

export { expect };
