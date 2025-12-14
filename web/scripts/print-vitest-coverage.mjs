#!/usr/bin/env node
/**
 * Prints Vitest coverage summary from coverage-summary.json
 * Used by CI to add coverage info to GitHub step summary
 */

import { readFileSync, existsSync } from 'fs';
import { join } from 'path';

const summaryPath = join(process.cwd(), 'coverage/vitest/coverage-summary.json');

if (!existsSync(summaryPath)) {
  console.log('Coverage summary not available');
  process.exit(0);
}

try {
  const data = JSON.parse(readFileSync(summaryPath, 'utf8'));
  const total = data.total;

  console.log(`Lines:      ${total.lines.pct}%`);
  console.log(`Branches:   ${total.branches.pct}%`);
  console.log(`Functions:  ${total.functions.pct}%`);
  console.log(`Statements: ${total.statements.pct}%`);
} catch (err) {
  console.log('Failed to parse coverage summary:', err.message);
  process.exit(1);
}
