#!/usr/bin/env node
/**
 * Builds a unified coverage HTML report from all test types.
 * Creates an index.html linking to individual coverage reports.
 *
 * Usage:
 *   node scripts/build-coverage-report.mjs \
 *     --vitest web/coverage/vitest \
 *     --playwright web/coverage/playwright \
 *     --rust service/coverage/backend-unit.lcov \
 *     --output coverage-report
 */

import { cpSync, existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'fs';
import { execSync } from 'child_process';
import { join, resolve } from 'path';
import { parseArgs } from 'util';

// Icon thresholds (matches summarize-coverage.mjs)
const getIcon = (pct) => {
  if (pct >= 80) {return { icon: '&#x1F7E2;', color: '#22c55e', label: 'good' };}
  if (pct >= 50) {return { icon: '&#x1F7E1;', color: '#eab308', label: 'ok' };}
  return { icon: '&#x1F534;', color: '#ef4444', label: 'low' };
};

// Parse command line args
// Note: Script runs from web/ directory to access monocart-coverage-reports
const { values } = parseArgs({
  options: {
    vitest: { type: 'string', default: 'coverage/vitest' },
    playwright: { type: 'string', default: 'coverage/playwright' },
    rust: { type: 'string', default: '../service/coverage/backend-unit.lcov' },
    output: { type: 'string', default: '../coverage-report' },
  },
});

// ============================================================================
// Coverage Parsing (simplified from summarize-coverage.mjs)
// ============================================================================

function parseVitestSummary(summaryPath) {
  if (!existsSync(summaryPath)) {return null;}
  try {
    const data = JSON.parse(readFileSync(summaryPath, 'utf8'));
    return data.total || null;
  } catch {
    return null;
  }
}

function parsePlaywrightSummary(coverageDir) {
  const summaryPath = join(coverageDir, 'coverage-summary.json');
  if (!existsSync(summaryPath)) {return null;}
  try {
    const data = JSON.parse(readFileSync(summaryPath, 'utf8'));
    return data.total || null;
  } catch {
    return null;
  }
}

function parseLcovSummary(lcovPath) {
  if (!existsSync(lcovPath)) {return null;}
  try {
    const content = readFileSync(lcovPath, 'utf8');
    let linesTotal = 0,
      linesCovered = 0;
    let funcsTotal = 0,
      funcsCovered = 0;

    for (const line of content.split('\n')) {
      if (line.startsWith('LF:')) {linesTotal += parseInt(line.substring(3), 10);}
      else if (line.startsWith('LH:')) {linesCovered += parseInt(line.substring(3), 10);}
      else if (line.startsWith('FNF:')) {funcsTotal += parseInt(line.substring(4), 10);}
      else if (line.startsWith('FNH:')) {funcsCovered += parseInt(line.substring(4), 10);}
    }

    const linesPct = linesTotal > 0 ? (linesCovered / linesTotal) * 100 : 0;
    const funcsPct = funcsTotal > 0 ? (funcsCovered / funcsTotal) * 100 : 0;

    return {
      lines: { pct: linesPct, covered: linesCovered, total: linesTotal },
      functions: { pct: funcsPct, covered: funcsCovered, total: funcsTotal },
    };
  } catch {
    return null;
  }
}

// ============================================================================
// HTML Generation
// ============================================================================

function generateIndexHtml(reports) {
  const getPct = (metric) => {
    if (!metric) {return 0;}
    if (typeof metric.pct === 'number') {return metric.pct;}
    if (typeof metric === 'number') {return metric;}
    return 0;
  };

  const rows = reports
    .filter((r) => r.summary)
    .map((r) => {
      const linesPct = getPct(r.summary.lines);
      const branchesPct = getPct(r.summary.branches);
      const funcsPct = getPct(r.summary.functions);

      const linesInfo = getIcon(linesPct);
      const branchesInfo = branchesPct !== undefined ? getIcon(branchesPct) : null;
      const funcsInfo = getIcon(funcsPct);

      return `
        <tr>
          <td>
            <a href="${r.dir}/index.html">${r.icon} ${r.name}</a>
          </td>
          <td style="color: ${linesInfo.color}">${linesPct.toFixed(1)}%</td>
          <td style="color: ${branchesInfo?.color || '#888'}">${branchesInfo ? `${branchesPct.toFixed(1)  }%` : 'N/A'}</td>
          <td style="color: ${funcsInfo.color}">${funcsPct.toFixed(1)}%</td>
          <td><a href="${r.dir}/index.html">View Report</a></td>
        </tr>`;
    })
    .join('\n');

  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Coverage Report</title>
  <style>
    :root {
      --bg: #1a1a2e;
      --surface: #16213e;
      --text: #e4e4e7;
      --muted: #a1a1aa;
      --border: #3f3f46;
      --accent: #818cf8;
    }
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body {
      font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
      background: var(--bg);
      color: var(--text);
      line-height: 1.6;
      padding: 2rem;
    }
    .container { max-width: 900px; margin: 0 auto; }
    h1 {
      font-size: 1.75rem;
      margin-bottom: 0.5rem;
    }
    .subtitle {
      color: var(--muted);
      margin-bottom: 2rem;
    }
    table {
      width: 100%;
      border-collapse: collapse;
      background: var(--surface);
      border-radius: 8px;
      overflow: hidden;
    }
    th, td {
      padding: 1rem;
      text-align: left;
      border-bottom: 1px solid var(--border);
    }
    th {
      background: rgba(0,0,0,0.2);
      font-weight: 600;
      color: var(--muted);
      font-size: 0.875rem;
      text-transform: uppercase;
      letter-spacing: 0.05em;
    }
    tr:last-child td { border-bottom: none; }
    tr:hover { background: rgba(255,255,255,0.02); }
    a {
      color: var(--accent);
      text-decoration: none;
    }
    a:hover { text-decoration: underline; }
    .legend {
      margin-top: 1.5rem;
      padding: 1rem;
      background: var(--surface);
      border-radius: 8px;
      font-size: 0.875rem;
      color: var(--muted);
    }
    .legend span { margin-right: 1.5rem; }
    .good { color: #22c55e; }
    .ok { color: #eab308; }
    .low { color: #ef4444; }
  </style>
</head>
<body>
  <div class="container">
    <h1>Coverage Report</h1>
    <p class="subtitle">Generated ${new Date().toISOString().split('T')[0]}</p>

    <table>
      <thead>
        <tr>
          <th>Test Type</th>
          <th>Lines</th>
          <th>Branches</th>
          <th>Functions</th>
          <th>Details</th>
        </tr>
      </thead>
      <tbody>
        ${rows}
      </tbody>
    </table>

    <div class="legend">
      <span class="good">&#x2022; &ge;80% Good</span>
      <span class="ok">&#x2022; &ge;50% Acceptable</span>
      <span class="low">&#x2022; &lt;50% Needs improvement</span>
    </div>
  </div>
</body>
</html>`;
}

// ============================================================================
// Main
// ============================================================================

async function main() {
  const outputDir = values.output;

  // Clean and create output directory
  mkdirSync(outputDir, { recursive: true });

  const reports = [];

  // Vitest coverage
  const vitestDir = values.vitest;
  if (existsSync(vitestDir) && existsSync(join(vitestDir, 'index.html'))) {
    console.log('Copying Vitest coverage...');
    cpSync(vitestDir, join(outputDir, 'vitest'), { recursive: true });
    reports.push({
      name: 'Vitest Unit Tests',
      icon: '&#x1F9EA;',
      dir: 'vitest',
      summary: parseVitestSummary(join(vitestDir, 'coverage-summary.json')),
    });
  } else {
    console.log('Vitest coverage not found, skipping...');
  }

  // Playwright coverage (Monocart outputs directly to coverage/playwright)
  const playwrightDir = values.playwright;
  // Check multiple possible HTML locations for compatibility
  const possibleHtmlDirs = [
    playwrightDir, // Monocart outputs here
    join(playwrightDir, 'lcov-report'),
    join(playwrightDir, 'html'),
  ];

  let playwrightHtmlDir = null;
  for (const dir of possibleHtmlDirs) {
    if (existsSync(join(dir, 'index.html'))) {
      playwrightHtmlDir = dir;
      break;
    }
  }

  if (playwrightHtmlDir) {
    console.log(`Copying Playwright coverage from ${playwrightHtmlDir}...`);
    cpSync(playwrightHtmlDir, join(outputDir, 'playwright'), { recursive: true });
    reports.push({
      name: 'Playwright E2E',
      icon: '&#x1F3AD;',
      dir: 'playwright',
      summary: parsePlaywrightSummary(playwrightDir),
    });
  } else {
    console.log(`Playwright HTML coverage not found. Checked: ${possibleHtmlDirs.join(', ')}`);
    // Debug: list what's actually in the coverage directory
    console.log(`Checking if ${playwrightDir} exists: ${existsSync(playwrightDir)}`);
    if (existsSync(playwrightDir)) {
      try {
        const contents = readdirSync(playwrightDir);
        console.log(`Contents of ${playwrightDir}: ${contents.length > 0 ? contents.join(', ') : '(empty)'}`);
      } catch (e) {
        console.log(`Could not list ${playwrightDir}: ${e.message}`);
      }
    }
  }

  // Rust coverage (generate HTML from LCOV using genhtml)
  const rustLcov = values.rust;
  if (existsSync(rustLcov)) {
    console.log(`Generating Rust coverage HTML from ${rustLcov}...`);
    const rustOutputDir = join(outputDir, 'rust');

    // Resolve absolute paths since we'll run genhtml from service directory
    const absoluteLcov = resolve(rustLcov);
    const absoluteOutputDir = resolve(rustOutputDir);
    const serviceDir = resolve('../service');

    try {
      // Run genhtml from service directory so it can find src/*.rs files
      execSync(`genhtml "${absoluteLcov}" --output-directory "${absoluteOutputDir}" --dark-mode --title "Rust Backend Coverage"`, {
        stdio: 'inherit',
        cwd: serviceDir,
      });

      // Verify genhtml created the index.html
      if (existsSync(join(rustOutputDir, 'index.html'))) {
        reports.push({
          name: 'Rust Backend',
          icon: '&#x1F980;',
          dir: 'rust',
          summary: parseLcovSummary(rustLcov),
        });
      } else {
        console.error('genhtml ran but did not create index.html');
      }
    } catch (err) {
      console.error('Failed to generate Rust HTML coverage.');
      console.error('Error:', err.message);
      console.error('Make sure lcov is installed (apt-get install lcov)');
    }
  } else {
    console.log(`Rust LCOV not found at ${rustLcov}, skipping...`);
  }

  // Generate index.html
  if (reports.length > 0) {
    console.log('Generating index.html...');
    writeFileSync(join(outputDir, 'index.html'), generateIndexHtml(reports));
    console.log(`Coverage report generated at ${outputDir}/index.html`);
    console.log(`Reports included: ${reports.map((r) => r.name).join(', ')}`);
  } else {
    console.log('No coverage data found. No report generated.');
    process.exit(1);
  }
}

main().catch((err) => {
  console.error('Error:', err.message);
  process.exit(1);
});
