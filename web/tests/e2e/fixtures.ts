import fs from 'node:fs/promises';
import path from 'node:path';
import { test as base, expect } from '@playwright/test';
import MCR from 'monocart-coverage-reports';

const truthy = (value: string | undefined) =>
  (value ?? '').toLowerCase() === 'true' || value === '1';
const shouldCollectCoverage = truthy(process.env.PLAYWRIGHT_COVERAGE) || truthy(process.env.CI);

// Coverage output directory
const coverageDir = path.join(process.cwd(), 'coverage/playwright');

// Create or get the shared coverage report instance
// We write raw coverage data per test, then generate report in global teardown
const rawCoverageDir = path.join(process.cwd(), '.playwright-coverage');

// Extended test fixture with V8 coverage collection via Monocart
// page.coverage (CDP) is only available on Chromium-based browsers.
export const test = base.extend<{ coveragePage: void }>({
  coveragePage: [
    async ({ page, browserName }, use, testInfo) => {
      const canCollectCoverage = shouldCollectCoverage && browserName === 'chromium';

      if (canCollectCoverage) {
        await page.coverage.startJSCoverage({ resetOnNavigation: false });
      }

      // Run the test
      await use();

      if (canCollectCoverage) {
        const coverage = await page.coverage.stopJSCoverage();

        if (coverage.length > 0) {
          // Write raw V8 coverage data for this test
          // The global teardown will merge and generate the report
          await fs.mkdir(rawCoverageDir, { recursive: true });
          const safeId = testInfo.testId.replace(/[^a-z0-9_-]/gi, '_');
          const filePath = path.join(rawCoverageDir, `${safeId}.json`);
          await fs.writeFile(filePath, JSON.stringify(coverage));
        }
      }
    },
    { auto: true },
  ],
});

// Generate coverage report from collected raw data
export async function generateCoverageReport(): Promise<void> {
  if (!shouldCollectCoverage) {
    return;
  }

  // Check if there's any raw coverage data
  try {
    await fs.access(rawCoverageDir);
  } catch {
    // eslint-disable-next-line no-console
    console.log('[Coverage] No raw coverage data found, skipping report generation');
    return;
  }

  const coverageReport = MCR({
    name: 'Playwright E2E Coverage',
    outputDir: coverageDir,
    reports: ['lcov', 'html', 'json-summary', 'text-summary'],

    // Filter coverage entries by URL before source map resolution
    entryFilter: (entry) => {
      // Only include scripts from our app (localhost), not external CDNs
      return entry.url.includes('localhost') || entry.url.includes('127.0.0.1');
    },

    // Filter source files after source map resolution
    sourceFilter: (sourcePath: string) => {
      // Only include our app source files, exclude node_modules
      if (sourcePath.includes('node_modules')) {
        return false;
      }
      return sourcePath.startsWith('src/');
    },
  });

  // Coverage thresholds — checked after report generation so a thrown error
  // actually propagates through Playwright's global teardown (process.exitCode
  // alone gets overridden by Playwright's own exit handling).
  const thresholds = {
    lines: 50,
    branches: 25,
    functions: 45,
    statements: 50,
  };

  // Read and add all raw coverage files
  const files = await fs.readdir(rawCoverageDir);
  for (const file of files) {
    if (file.endsWith('.json')) {
      const filePath = path.join(rawCoverageDir, file);
      const content = await fs.readFile(filePath, 'utf-8');
      const coverage = JSON.parse(content);
      await coverageReport.add(coverage);
    }
  }

  // Generate the merged report
  await coverageReport.generate();

  // Clean up raw coverage files
  await fs.rm(rawCoverageDir, { recursive: true, force: true });

  // Check thresholds after generation — read the json-summary output
  const summaryPath = path.join(coverageDir, 'coverage-summary.json');
  try {
    const summaryJson = JSON.parse(await fs.readFile(summaryPath, 'utf-8'));
    const total = summaryJson.total;
    const errors: string[] = [];

    for (const [metric, threshold] of Object.entries(thresholds)) {
      const pct: number = total?.[metric]?.pct ?? 0;
      if (pct < threshold) {
        errors.push(`  ${metric}: ${pct.toFixed(1)}% < ${String(threshold)}%`);
      }
    }

    if (errors.length > 0) {
      throw new Error(`E2E coverage thresholds not met:\n${errors.join('\n')}`);
    }
  } catch (err) {
    // Re-throw threshold errors, but don't fail if summary file is missing
    if (err instanceof Error && err.message.startsWith('E2E coverage')) {
      throw err;
    }
  }
}

export type { Page } from '@playwright/test';
export { expect };
