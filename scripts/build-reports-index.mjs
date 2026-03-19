#!/usr/bin/env node
// scripts/build-reports-index.mjs
//
// Generates/updates the root build-history index for reports.ibcook.com.
// Reads an existing index.html to preserve history, prepends a new build
// entry, and caps at MAX_ENTRIES.
//
// Usage: node scripts/build-reports-index.mjs [--existing path] --output path
// Required env: GITHUB_SHA, GITHUB_REF_NAME, GITHUB_RUN_ID, GITHUB_RUN_NUMBER,
//               COVERAGE_PCT, BUILD_DATE

import { readFileSync, writeFileSync } from 'node:fs';
import { parseArgs } from 'node:util';

const MAX_ENTRIES = 50;

const { values } = parseArgs({
  options: {
    existing: { type: 'string' },
    output: { type: 'string' },
  },
});

if (!values.output) {
  console.error('Usage: build-reports-index.mjs [--existing path] --output path');
  process.exit(1);
}

// ---------------------------------------------------------------------------
// Extract build entries from existing index (embedded JSON in HTML comment)
// ---------------------------------------------------------------------------
function extractBuilds(html) {
  const match = html.match(/<!--BUILDS_DATA([\s\S]*?)BUILDS_DATA-->/);
  if (!match) return [];
  try {
    return JSON.parse(match[1]);
  } catch {
    return [];
  }
}

// ---------------------------------------------------------------------------
// Build the new entry from env vars
// ---------------------------------------------------------------------------
const sha = process.env.GITHUB_SHA ?? '';
const newBuild = {
  sha,
  short: sha.slice(0, 7) || '???????',
  branch: process.env.GITHUB_REF_NAME ?? 'unknown',
  date: process.env.BUILD_DATE ?? new Date().toISOString().replace('T', ' ').slice(0, 19) + ' UTC',
  coverage: process.env.COVERAGE_PCT ?? '?',
  runId: process.env.GITHUB_RUN_ID ?? '',
  runNumber: process.env.GITHUB_RUN_NUMBER ?? '',
};

// ---------------------------------------------------------------------------
// Load existing builds, prepend new, deduplicate, cap
// ---------------------------------------------------------------------------
let builds = [];
if (values.existing) {
  try {
    builds = extractBuilds(readFileSync(values.existing, 'utf-8'));
  } catch {
    // File doesn't exist or unreadable — start fresh
  }
}

builds = [newBuild, ...builds.filter((b) => b.sha !== sha)].slice(0, MAX_ENTRIES);

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------
function coverageClass(pct) {
  const n = parseFloat(pct);
  if (Number.isNaN(n)) return '';
  if (n >= 80) return ' good';
  if (n >= 50) return ' ok';
  return ' low';
}

const rows = builds
  .map(
    (b, i) => `        <tr${i === 0 ? ' class="current"' : ''}>
          <td class="mono"><a href="${b.sha}/index.html">${b.short}</a></td>
          <td>${b.branch}</td>
          <td class="pct${coverageClass(b.coverage)}">${b.coverage}%</td>
          <td class="mono">${b.date}</td>
          <td><a href="https://github.com/icook/tiny-congress/actions/runs/${b.runId}">#${b.runNumber}</a></td>
        </tr>`
  )
  .join('\n');

const html = `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Tiny Congress - Build Reports</title>
  <meta name="color-scheme" content="light dark">
  <style>
    :root {
      --bg: #f8fafc; --bg-card: #ffffff; --bg-hover: #f1f5f9;
      --text: #0f172a; --text-secondary: #64748b; --text-muted: #94a3b8;
      --link: #2563eb; --border: #e2e8f0; --accent: #3b82f6;
      --shadow: 0 1px 3px rgba(0,0,0,0.1), 0 1px 2px rgba(0,0,0,0.06);
    }
    @media (prefers-color-scheme: dark) {
      :root {
        --bg: #0f172a; --bg-card: #1e293b; --bg-hover: #334155;
        --text: #f1f5f9; --text-secondary: #94a3b8; --text-muted: #64748b;
        --link: #60a5fa; --border: #334155; --accent: #3b82f6;
        --shadow: 0 1px 3px rgba(0,0,0,0.3);
      }
    }
    * { box-sizing: border-box; }
    body {
      font-family: system-ui, -apple-system, BlinkMacSystemFont, sans-serif;
      max-width: 960px; margin: 0 auto; padding: 24px;
      line-height: 1.6; background: var(--bg); color: var(--text);
    }
    .header {
      background: var(--bg-card); border-radius: 12px; padding: 24px;
      margin-bottom: 24px; box-shadow: var(--shadow); border: 1px solid var(--border);
      display: flex; align-items: center; gap: 16px;
    }
    .header h1 { margin: 0; font-size: 1.5rem; font-weight: 600; }
    .header .subtitle { color: var(--text-muted); font-size: 0.875rem; }
    .section {
      background: var(--bg-card); border-radius: 12px; padding: 20px;
      box-shadow: var(--shadow); border: 1px solid var(--border);
    }
    .section h2 {
      margin: 0 0 16px 0; font-size: 1rem; font-weight: 600;
      color: var(--text-secondary);
    }
    table { width: 100%; border-collapse: collapse; font-size: 0.875rem; }
    th {
      text-align: left; padding: 10px 12px; background: var(--bg-hover);
      color: var(--text-muted); font-weight: 500; text-transform: uppercase;
      font-size: 0.75rem; letter-spacing: 0.05em;
    }
    td { padding: 10px 12px; border-bottom: 1px solid var(--border); }
    tr:last-child td { border-bottom: none; }
    tr.current td { background: color-mix(in srgb, var(--accent) 8%, transparent); }
    a { color: var(--link); text-decoration: none; }
    a:hover { text-decoration: underline; }
    .mono { font-family: ui-monospace, 'SF Mono', monospace; font-size: 0.8125rem; }
    .pct { font-weight: 600; font-family: ui-monospace, monospace; }
    .pct.good { color: #22c55e; }
    .pct.ok { color: #eab308; }
    .pct.low { color: #ef4444; }
  </style>
</head>
<body>
  <header class="header">
    <div>
      <h1>Tiny Congress</h1>
      <div class="subtitle">Build Reports &mdash; last ${builds.length} builds</div>
    </div>
  </header>
  <section class="section">
    <h2>Recent Builds</h2>
    <table>
      <thead>
        <tr>
          <th>Commit</th>
          <th>Branch</th>
          <th>Coverage</th>
          <th>Built</th>
          <th>Workflow</th>
        </tr>
      </thead>
      <tbody>
${rows}
      </tbody>
    </table>
  </section>
  <!--BUILDS_DATA${JSON.stringify(builds)}BUILDS_DATA-->
</body>
</html>
`;

writeFileSync(values.output, html);
console.log(`Wrote ${builds.length} builds to ${values.output}`);
