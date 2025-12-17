import { generateCoverageReport } from './fixtures';

export default async function globalTeardown(): Promise<void> {
  await generateCoverageReport();
}
