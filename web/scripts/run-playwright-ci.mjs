import { spawn } from 'node:child_process';
import os from 'node:os';

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

async function main() {
  const coverageEnv = { ...process.env, PLAYWRIGHT_COVERAGE: '1' };

  // Clean previous coverage artifacts
  const cleanCode = await run('yarn', ['playwright:clean'], { env: process.env });
  if (cleanCode !== 0) {
    process.exit(cleanCode);
  }

  // Run tests - Monocart generates coverage report in global teardown
  const testExitCode = await run('yarn', ['playwright:test'], { env: coverageEnv });

  process.exit(testExitCode);
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
