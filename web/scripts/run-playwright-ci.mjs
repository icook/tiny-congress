import { spawn } from 'node:child_process';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';

async function run(command, args, options = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      stdio: 'inherit',
      shell: false,
      ...options,
    });

    child.on('error', reject);
    child.on('close', (code, signal) => {
      if (signal) {
        const signalNumber = os.constants.signals[signal];
        const signalExitCode = signalNumber ? 128 + signalNumber : 128;
        resolve(signalExitCode);
        return;
      }

      resolve(code ?? 0);
    });
  });
}

async function pathExists(target) {
  try {
    await fs.access(target);
    return true;
  } catch {
    return false;
  }
}

async function main() {
  const rootEnv = { ...process.env };
  const coverageEnv = { ...rootEnv, PLAYWRIGHT_COVERAGE: '1' };

  // Ensure previous reports don't leak into the current run.
  const resetCode = await run('node', ['./scripts/reset-playwright-coverage.mjs'], { env: rootEnv });
  if (resetCode !== 0) {
    process.exit(resetCode);
  }

  const testExitCode = await run('yarn', ['playwright:test'], { env: coverageEnv });

  const nycOutputPath = path.join(process.cwd(), '.nyc_output');
  if (await pathExists(nycOutputPath)) {
    const reportCode = await run('yarn', ['playwright:report'], { env: rootEnv });
    if (reportCode !== 0 && testExitCode === 0) {
      process.exit(reportCode);
    }
  }

  process.exit(testExitCode);
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
