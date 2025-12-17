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
export const test = base.extend<{ coveragePage: void }>({
  coveragePage: [
    async ({ page }, use, testInfo) => {
      if (shouldCollectCoverage) {
        await page.coverage.startJSCoverage({ resetOnNavigation: false });
      }

      // Run the test
      await use();

      if (shouldCollectCoverage) {
        const coverage = await page.coverage.stopJSCoverage();

        // eslint-disable-next-line no-console
        console.log(`[Coverage] Test "${testInfo.title}" collected ${coverage.length} scripts`);
        if (coverage.length > 0) {
          // eslint-disable-next-line no-console
          console.log(`[Coverage] Scripts: ${coverage.map((c) => c.url).join(', ')}`);
        }

        if (coverage.length > 0) {
          // Write raw V8 coverage data for this test
          // The global teardown will merge and generate the report
          await fs.mkdir(rawCoverageDir, { recursive: true });
          const safeId = testInfo.testId.replace(/[^a-z0-9_-]/gi, '_');
          const filePath = path.join(rawCoverageDir, `${safeId}.json`);
          await fs.writeFile(filePath, JSON.stringify(coverage));
          // eslint-disable-next-line no-console
          console.log(`[Coverage] Wrote raw data to: ${filePath}`);
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
      // Exclude node_modules and only include project source files
      if (sourcePath.includes('node_modules')) {
        return false;
      }
      // Include src files (after source map resolution) or built assets
      return sourcePath.includes('/src/') || sourcePath.includes('/assets/');
    },
  });

  // Read and add all raw coverage files
  const files = await fs.readdir(rawCoverageDir);
  // eslint-disable-next-line no-console
  console.log(`[Coverage] Found ${files.length} raw coverage files`);

  for (const file of files) {
    if (file.endsWith('.json')) {
      const filePath = path.join(rawCoverageDir, file);
      const content = await fs.readFile(filePath, 'utf-8');
      const coverage = JSON.parse(content);
      // eslint-disable-next-line no-console
      console.log(`[Coverage] Adding coverage from ${file} (${coverage.length} entries)`);
      await coverageReport.add(coverage);
    }
  }

  // Generate the merged report
  await coverageReport.generate();

  // Clean up raw coverage files
  await fs.rm(rawCoverageDir, { recursive: true, force: true });
}

export { expect };
