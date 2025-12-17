#!/usr/bin/env node
/**
 * Analyzes Playwright test results and reports flaky tests.
 * A test is considered "flaky" if it failed initially but passed on retry.
 *
 * Usage: node scripts/analyze-playwright-flakiness.mjs [results-file]
 *
 * Output format (GitHub Actions):
 * - Writes markdown summary to stdout for GITHUB_STEP_SUMMARY
 * - Sets FLAKY_COUNT output variable
 */

import { readFileSync, existsSync } from 'fs';

const resultsFile = process.argv[2] || 'reports/playwright-results.json';

if (!existsSync(resultsFile)) {
  console.log('No Playwright results file found. Skipping flakiness analysis.');
  process.exit(0);
}

const results = JSON.parse(readFileSync(resultsFile, 'utf-8'));

// Collect flaky tests (tests that have retry > 0 and eventually passed)
const flakyTests = [];
const failedTests = [];

for (const suite of results.suites || []) {
  collectFlakyTests(suite, flakyTests, failedTests);
}

function collectFlakyTests(suite, flaky, failed, path = []) {
  const currentPath = suite.title ? [...path, suite.title] : path;

  for (const spec of suite.specs || []) {
    for (const test of spec.tests || []) {
      const testPath = [...currentPath, spec.title].join(' > ');
      const results = test.results || [];

      // Check if test was retried
      if (results.length > 1) {
        const lastResult = results[results.length - 1];
        const hadFailure = results.slice(0, -1).some(r => r.status === 'failed');

        if (hadFailure && lastResult.status === 'passed') {
          flaky.push({
            name: testPath,
            retries: results.length - 1,
            duration: results.reduce((sum, r) => sum + (r.duration || 0), 0),
          });
        } else if (lastResult.status === 'failed') {
          failed.push({
            name: testPath,
            retries: results.length - 1,
          });
        }
      }
    }
  }

  for (const child of suite.suites || []) {
    collectFlakyTests(child, flaky, failed, currentPath);
  }
}

// Generate report
const totalTests = results.stats?.expected || 0;
const flakyCount = flakyTests.length;

console.log('### Playwright Test Stability');
console.log('');

if (flakyCount === 0) {
  console.log('✅ No flaky tests detected');
} else {
  console.log(`⚠️ **${flakyCount} flaky test(s) detected** (passed on retry)`);
  console.log('');
  console.log('| Test | Retries | Total Duration |');
  console.log('|------|---------|----------------|');
  for (const test of flakyTests) {
    console.log(`| ${test.name} | ${test.retries} | ${test.duration}ms |`);
  }
  console.log('');
  console.log('> Flaky tests should be investigated and fixed. See `docs/playbooks/fixing-flaky-tests.md`');
}

// Output for GitHub Actions
if (process.env.GITHUB_OUTPUT) {
  const { appendFileSync } = await import('fs');
  appendFileSync(process.env.GITHUB_OUTPUT, `flaky_count=${flakyCount}\n`);
}

// Exit with warning code if flaky tests found (non-blocking)
process.exit(0);
