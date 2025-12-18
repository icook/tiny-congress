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

import { copyFileSync, cpSync, existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'fs';
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
// Schema Viewer Generation
// ============================================================================

function generateSwaggerUI() {
  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>OpenAPI Specification</title>
  <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5/swagger-ui.css">
  <style>
    body { margin: 0; padding: 0; }
    .swagger-ui .topbar { display: none; }
    .back-link {
      position: fixed;
      top: 10px;
      left: 10px;
      z-index: 1000;
      background: #1a1a2e;
      color: #818cf8;
      padding: 8px 16px;
      border-radius: 4px;
      text-decoration: none;
      font-family: system-ui, sans-serif;
      font-size: 14px;
    }
    .back-link:hover { background: #16213e; }
  </style>
</head>
<body>
  <a href="../index.html" class="back-link">&larr; Back to Coverage Report</a>
  <div id="swagger-ui"></div>
  <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js"></script>
  <script>
    SwaggerUIBundle({
      url: 'openapi.json',
      dom_id: '#swagger-ui',
      deepLinking: true,
      presets: [SwaggerUIBundle.presets.apis, SwaggerUIBundle.SwaggerUIStandalonePreset],
      layout: 'BaseLayout'
    });
  </script>
</body>
</html>`;
}

function generateGraphQLViewer(schemaContent) {
  // Escape HTML entities in schema content
  const escaped = schemaContent
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');

  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>GraphQL Schema</title>
  <link rel="stylesheet" href="https://unpkg.com/prismjs@1/themes/prism-tomorrow.min.css">
  <style>
    :root {
      --bg: #1a1a2e;
      --surface: #16213e;
      --text: #e4e4e7;
      --muted: #a1a1aa;
      --accent: #818cf8;
    }
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body {
      font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
      background: var(--bg);
      color: var(--text);
      line-height: 1.6;
    }
    .header {
      background: var(--surface);
      padding: 1rem 2rem;
      display: flex;
      align-items: center;
      justify-content: space-between;
      border-bottom: 1px solid #3f3f46;
      position: sticky;
      top: 0;
      z-index: 100;
    }
    .header h1 {
      font-size: 1.25rem;
      display: flex;
      align-items: center;
      gap: 0.5rem;
    }
    .header-actions {
      display: flex;
      gap: 1rem;
    }
    .header a {
      color: var(--accent);
      text-decoration: none;
      font-size: 0.875rem;
    }
    .header a:hover { text-decoration: underline; }
    .schema-container {
      padding: 1rem 2rem 2rem;
      max-width: 100%;
      overflow-x: auto;
    }
    pre[class*="language-"] {
      background: var(--surface) !important;
      border-radius: 8px;
      padding: 1.5rem !important;
      margin: 0 !important;
      font-size: 0.875rem;
      line-height: 1.7;
    }
    code[class*="language-"] {
      font-family: 'SF Mono', 'Fira Code', 'Monaco', monospace;
    }
    /* GraphQL syntax highlighting overrides */
    .token.keyword { color: #c792ea; }
    .token.type-def { color: #ffcb6b; }
    .token.directive { color: #89ddff; }
    .token.comment { color: #676e95; }
    .token.string { color: #c3e88d; }
    .token.punctuation { color: #89ddff; }
  </style>
</head>
<body>
  <div class="header">
    <h1>&#x1F4DC; GraphQL Schema</h1>
    <div class="header-actions">
      <a href="schema.graphql" download>Download SDL</a>
      <a href="../index.html">&larr; Back to Coverage Report</a>
    </div>
  </div>
  <div class="schema-container">
    <pre><code class="language-graphql">${escaped}</code></pre>
  </div>
  <script src="https://unpkg.com/prismjs@1/components/prism-core.min.js"></script>
  <script src="https://unpkg.com/prismjs@1/components/prism-graphql.min.js"></script>
</body>
</html>`;
}

// ============================================================================
// HTML Generation
// ============================================================================

function generateIndexHtml(reports, schemas = []) {
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

  const schemaRows = schemas
    .map((s) => `
        <tr>
          <td>${s.icon} ${s.name}</td>
          <td>${s.description}</td>
          <td><a href="${s.path}">Browse</a> | <a href="${s.rawPath}" target="_blank">Raw</a> | <a href="${s.rawPath}" download>Download</a></td>
        </tr>`)
    .join('\n');

  const schemasSection = schemas.length > 0 ? `
    <h2>API Schemas</h2>
    <table class="schemas-table">
      <thead>
        <tr>
          <th>Schema</th>
          <th>Description</th>
          <th>Actions</th>
        </tr>
      </thead>
      <tbody>
        ${schemaRows}
      </tbody>
    </table>
  ` : '';

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
    h2 {
      font-size: 1.25rem;
      margin-top: 2rem;
      margin-bottom: 1rem;
      color: var(--text);
    }
    .schemas-table {
      margin-bottom: 1.5rem;
    }
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

    ${schemasSection}

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

  // Copy API schemas and generate viewers
  const schemas = [];
  const schemasDir = join(outputDir, 'schemas');

  const graphqlSchema = 'schema.graphql';
  const openapiSchema = 'openapi.json';

  if (existsSync(graphqlSchema) || existsSync(openapiSchema)) {
    mkdirSync(schemasDir, { recursive: true });
    console.log('Copying API schemas and generating viewers...');

    if (existsSync(graphqlSchema)) {
      copyFileSync(graphqlSchema, join(schemasDir, 'schema.graphql'));
      const graphqlContent = readFileSync(graphqlSchema, 'utf8');
      writeFileSync(join(schemasDir, 'graphql-viewer.html'), generateGraphQLViewer(graphqlContent));
      schemas.push({
        name: 'GraphQL Schema',
        icon: '&#x1F4DC;',
        description: 'GraphQL SDL schema definition',
        path: 'schemas/graphql-viewer.html',
        rawPath: 'schemas/schema.graphql',
      });
      console.log('  - schema.graphql + viewer');
    }

    if (existsSync(openapiSchema)) {
      copyFileSync(openapiSchema, join(schemasDir, 'openapi.json'));
      writeFileSync(join(schemasDir, 'swagger-ui.html'), generateSwaggerUI());
      schemas.push({
        name: 'OpenAPI Spec',
        icon: '&#x1F4CB;',
        description: 'REST API specification (OpenAPI 3.0)',
        path: 'schemas/swagger-ui.html',
        rawPath: 'schemas/openapi.json',
      });
      console.log('  - openapi.json + Swagger UI');
    }
  } else {
    console.log('No API schemas found, skipping...');
  }

  // Generate index.html
  if (reports.length > 0) {
    console.log('Generating index.html...');
    writeFileSync(join(outputDir, 'index.html'), generateIndexHtml(reports, schemas));
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
