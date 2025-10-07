import fs from 'node:fs/promises';
import path from 'node:path';
import { test as base, expect } from '@playwright/test';

const truthy = (value: string | undefined) =>
  (value ?? '').toLowerCase() === 'true' || value === '1';
const shouldCollectCoverage = truthy(process.env.PLAYWRIGHT_COVERAGE) || truthy(process.env.CI);

if (shouldCollectCoverage) {
  base.afterEach(async ({ context }, testInfo) => {
    const coverageDir = path.join(process.cwd(), '.nyc_output');
    await fs.mkdir(coverageDir, { recursive: true });

    const pages = context.pages();
    await Promise.all(
      pages.map(async (page, index) => {
        try {
          const coverage = await page.evaluate(() => {
            const snapshot = (globalThis as any).__coverage__;
            (globalThis as any).__coverage__ = undefined;
            return snapshot;
          });

          if (!coverage) {
            return;
          }

          const safeId = testInfo.testId.replace(/[^a-z0-9_-]/gi, '_');
          const filePath = path.join(
            coverageDir,
            `${safeId}-worker${testInfo.workerIndex}-retry${testInfo.retry}-page${index}.json`
          );
          await fs.writeFile(filePath, JSON.stringify(coverage));
        } catch (error) {
          // Swallow evaluation errors for closed pages.
        }
      })
    );
  });
}

export const test = base;
export { expect };
