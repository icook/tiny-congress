import fs from 'node:fs/promises';
import path from 'node:path';

const targets = [
  '.playwright-coverage', // Raw V8 coverage data from monocart
  'coverage/playwright',
  'reports/playwright.xml',
  'playwright-report',
  'test-results',
];

async function removeTarget(target) {
  const location = path.join(process.cwd(), target);
  try {
    await fs.rm(location, { recursive: true, force: true });
  } catch {
    // Ignore removal errors.
  }
}

for (const target of targets) {
  await removeTarget(target);
}
