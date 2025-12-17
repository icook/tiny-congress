import fs from 'node:fs/promises';
import path from 'node:path';
import { test as base, expect } from '@playwright/test';

const truthy = (value: string | undefined) =>
  (value ?? '').toLowerCase() === 'true' || value === '1';
const shouldCollectCoverage = truthy(process.env.PLAYWRIGHT_COVERAGE) || truthy(process.env.CI);

if (shouldCollectCoverage) {
  // eslint-disable-next-line no-console
  console.log('[Coverage] Hook registered, will collect coverage after each test');
  base.afterEach(async ({ context }, testInfo) => {
    const coverageDir = path.join(process.cwd(), '.nyc_output');
    await fs.mkdir(coverageDir, { recursive: true });

    const pages = context.pages();
    // eslint-disable-next-line no-console
    console.log(`[Coverage] Collecting from ${pages.length} page(s) for test: ${testInfo.title}`);

    await Promise.all(
      pages.map(async (page, index) => {
        try {
          const coverageInfo = await page.evaluate(() => {
            const cov = (globalThis as any).__coverage__;
            return {
              exists: cov !== undefined,
              type: typeof cov,
              keyCount: cov ? Object.keys(cov).length : 0,
            };
          });
          // eslint-disable-next-line no-console
          console.log(
            `[Coverage] Page ${index}: exists=${coverageInfo.exists}, type=${coverageInfo.type}, keys=${coverageInfo.keyCount}`
          );

          const coverage = await page.evaluate(() => {
            const snapshot = (globalThis as any).__coverage__;
            (globalThis as any).__coverage__ = undefined;
            return snapshot;
          });

          if (!coverage) {
            // eslint-disable-next-line no-console
            console.log(`[Coverage] Page ${index}: No coverage data to collect`);
            return;
          }

          const safeId = testInfo.testId.replace(/[^a-z0-9_-]/gi, '_');
          const filePath = path.join(
            coverageDir,
            `${safeId}-worker${testInfo.workerIndex}-retry${testInfo.retry}-page${index}.json`
          );
          await fs.writeFile(filePath, JSON.stringify(coverage));
          // eslint-disable-next-line no-console
          console.log(`[Coverage] Page ${index}: Wrote coverage to ${filePath}`);
        } catch (error) {
          // eslint-disable-next-line no-console
          console.log(`[Coverage] Page ${index}: Error - ${error}`);
          // Swallow evaluation errors for closed pages.
        }
      })
    );
  });
}

export const test = base;
export { expect };
