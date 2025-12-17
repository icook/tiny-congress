#!/usr/bin/env node
/**
 * Unified coverage summary script for CI
 * Parses Vitest JSON, NYC/Istanbul JSON, and LCOV formats
 * Outputs unified markdown with directory-level breakdowns and icons
 *
 * Usage:
 *   node scripts/summarize-coverage.mjs \
 *     --vitest web/coverage/vitest/coverage-summary.json \
 *     --playwright web/.nyc_output \
 *     --rust service/coverage/backend-unit.lcov:unit,service/coverage/backend-integration.lcov:integration
 */

import { readFileSync, readdirSync, existsSync } from 'fs';
import { join, dirname, basename, relative } from 'path';
import { parseArgs } from 'util';

// Icon thresholds
const getIcon = (pct) => {
  if (pct >= 80) return '🟢';
  if (pct >= 50) return '🟡';
  return '🔴';
};

// Extract percentage from various coverage metric formats
// Handles: { pct: 50 }, { percent: 50 }, 50, "50"
const getPct = (metric) => {
  if (!metric) return 0;
  if (typeof metric === 'number') return metric;
  if (typeof metric === 'string') return Number(metric) || 0;
  if (typeof metric.pct === 'number') return metric.pct;
  if (typeof metric.percent === 'number') return metric.percent;
  // Calculate from covered/total if available
  if (metric.total > 0 && typeof metric.covered === 'number') {
    return (metric.covered / metric.total) * 100;
  }
  return 0;
};

const formatPct = (pct, bold = false) => {
  // Handle various input formats safely
  const numPct = typeof pct === 'number' ? pct : Number(pct) || 0;
  const icon = getIcon(numPct);
  const value = `${icon} ${numPct.toFixed(0)}%`;
  return bold ? `**${value}**` : value;
};

// Parse command line args
const { values } = parseArgs({
  options: {
    vitest: { type: 'string' },
    playwright: { type: 'string' },
    rust: { type: 'string' },
  },
});

// ============================================================================
// Vitest JSON Parser
// ============================================================================
function parseVitestCoverage(filePath) {
  if (!existsSync(filePath)) return null;

  const data = JSON.parse(readFileSync(filePath, 'utf8'));
  const dirStats = new Map();

  // Find the web root by looking at paths
  let webRoot = '';
  for (const key of Object.keys(data)) {
    if (key !== 'total' && key.includes('/web/src/')) {
      const idx = key.indexOf('/web/src/');
      webRoot = key.substring(0, idx + 5); // includes /web/
      break;
    }
  }

  for (const [filePath, stats] of Object.entries(data)) {
    if (filePath === 'total') continue;

    // Get relative path from web root
    let relPath = filePath;
    if (webRoot && filePath.startsWith(webRoot)) {
      relPath = filePath.substring(webRoot.length);
    }

    // Extract directory (first 2 levels: src/components, src/hooks, etc.)
    const parts = relPath.split('/').filter(Boolean);
    let dir = parts.length >= 2 ? `${parts[0]}/${parts[1]}` : parts[0] || 'root';

    if (!dirStats.has(dir)) {
      dirStats.set(dir, {
        lines: { covered: 0, total: 0 },
        branches: { covered: 0, total: 0 },
        functions: { covered: 0, total: 0 },
      });
    }

    const dirData = dirStats.get(dir);
    dirData.lines.covered += stats.lines.covered;
    dirData.lines.total += stats.lines.total;
    dirData.branches.covered += stats.branches.covered;
    dirData.branches.total += stats.branches.total;
    dirData.functions.covered += stats.functions.covered;
    dirData.functions.total += stats.functions.total;
  }

  return {
    total: data.total,
    byDirectory: dirStats,
  };
}

// ============================================================================
// Playwright Coverage Parser (supports both JSON summary and legacy NYC format)
// ============================================================================
function parsePlaywrightCoverage(inputPath) {
  if (!existsSync(inputPath)) return null;

  // If it's a JSON file, parse it like vitest (coverage-summary.json format)
  if (inputPath.endsWith('.json')) {
    return parseVitestCoverage(inputPath);
  }

  // Legacy: directory with NYC/Istanbul JSON files
  const files = readdirSync(inputPath).filter((f) => f.endsWith('.json'));
  if (files.length === 0) return null;

  const dirStats = new Map();
  let totalLines = { covered: 0, total: 0 };
  let totalBranches = { covered: 0, total: 0 };
  let totalFunctions = { covered: 0, total: 0 };

  // Find web root
  let webRoot = '';

  for (const file of files) {
    try {
      const data = JSON.parse(readFileSync(join(inputPath, file), 'utf8'));

      for (const [filePath, coverage] of Object.entries(data)) {
        // Find web root from first file
        if (!webRoot && filePath.includes('/web/src/')) {
          const idx = filePath.indexOf('/web/src/');
          webRoot = filePath.substring(0, idx + 5);
        }

        // Get relative path
        let relPath = filePath;
        if (webRoot && filePath.startsWith(webRoot)) {
          relPath = filePath.substring(webRoot.length);
        }

        // Extract directory
        const parts = relPath.split('/').filter(Boolean);
        let dir = parts.length >= 2 ? `${parts[0]}/${parts[1]}` : parts[0] || 'root';

        if (!dirStats.has(dir)) {
          dirStats.set(dir, {
            lines: { covered: 0, total: 0 },
            branches: { covered: 0, total: 0 },
            functions: { covered: 0, total: 0 },
          });
        }

        const dirData = dirStats.get(dir);

        // Count statements (lines)
        if (coverage.s) {
          const stmtValues = Object.values(coverage.s);
          const covered = stmtValues.filter((v) => v > 0).length;
          dirData.lines.covered += covered;
          dirData.lines.total += stmtValues.length;
          totalLines.covered += covered;
          totalLines.total += stmtValues.length;
        }

        // Count branches
        if (coverage.b) {
          for (const branchHits of Object.values(coverage.b)) {
            const covered = branchHits.filter((v) => v > 0).length;
            dirData.branches.covered += covered;
            dirData.branches.total += branchHits.length;
            totalBranches.covered += covered;
            totalBranches.total += branchHits.length;
          }
        }

        // Count functions
        if (coverage.f) {
          const funcValues = Object.values(coverage.f);
          const covered = funcValues.filter((v) => v > 0).length;
          dirData.functions.covered += covered;
          dirData.functions.total += funcValues.length;
          totalFunctions.covered += covered;
          totalFunctions.total += funcValues.length;
        }
      }
    } catch {
      // Skip malformed files
    }
  }

  if (dirStats.size === 0) return null;

  const calcPct = (stat) => (stat.total > 0 ? (stat.covered / stat.total) * 100 : 0);

  return {
    total: {
      lines: { pct: calcPct(totalLines), covered: totalLines.covered, total: totalLines.total },
      branches: {
        pct: calcPct(totalBranches),
        covered: totalBranches.covered,
        total: totalBranches.total,
      },
      functions: {
        pct: calcPct(totalFunctions),
        covered: totalFunctions.covered,
        total: totalFunctions.total,
      },
    },
    byDirectory: dirStats,
  };
}

// ============================================================================
// LCOV Parser
// ============================================================================
function parseLcovFile(filePath) {
  if (!existsSync(filePath)) return null;

  const content = readFileSync(filePath, 'utf8');
  const moduleStats = new Map();

  let currentFile = null;
  let currentLines = { covered: 0, total: 0 };
  let currentFunctions = { covered: 0, total: 0 };

  for (const line of content.split('\n')) {
    if (line.startsWith('SF:')) {
      currentFile = line.substring(3);
      currentLines = { covered: 0, total: 0 };
      currentFunctions = { covered: 0, total: 0 };
    } else if (line.startsWith('DA:')) {
      // DA:line_number,hit_count
      const [, hits] = line.substring(3).split(',');
      currentLines.total++;
      if (parseInt(hits, 10) > 0) currentLines.covered++;
    } else if (line.startsWith('FN:')) {
      // FN:line_number,function_name - function definition
      currentFunctions.total++;
    } else if (line.startsWith('FNDA:')) {
      // FNDA:hit_count,function_name
      const [hits] = line.substring(5).split(',');
      if (parseInt(hits, 10) > 0) currentFunctions.covered++;
    } else if (line === 'end_of_record' && currentFile) {
      // Extract module name from path
      // e.g., /path/to/service/src/api/mod.rs -> api
      let moduleName = 'other';
      const srcIdx = currentFile.indexOf('/src/');
      if (srcIdx !== -1) {
        const afterSrc = currentFile.substring(srcIdx + 5);
        const parts = afterSrc.split('/');
        moduleName = parts[0] || 'root';
      }

      if (!moduleStats.has(moduleName)) {
        moduleStats.set(moduleName, {
          lines: { covered: 0, total: 0 },
          functions: { covered: 0, total: 0 },
        });
      }

      const modData = moduleStats.get(moduleName);
      modData.lines.covered += currentLines.covered;
      modData.lines.total += currentLines.total;
      modData.functions.covered += currentFunctions.covered;
      modData.functions.total += currentFunctions.total;

      currentFile = null;
    }
  }

  if (moduleStats.size === 0) return null;

  // Calculate totals
  let totalLines = { covered: 0, total: 0 };
  let totalFunctions = { covered: 0, total: 0 };

  for (const stats of moduleStats.values()) {
    totalLines.covered += stats.lines.covered;
    totalLines.total += stats.lines.total;
    totalFunctions.covered += stats.functions.covered;
    totalFunctions.total += stats.functions.total;
  }

  const calcPct = (stat) => (stat.total > 0 ? (stat.covered / stat.total) * 100 : 0);

  return {
    total: {
      lines: { pct: calcPct(totalLines), covered: totalLines.covered, total: totalLines.total },
      functions: {
        pct: calcPct(totalFunctions),
        covered: totalFunctions.covered,
        total: totalFunctions.total,
      },
    },
    byModule: moduleStats,
  };
}

// ============================================================================
// Markdown Output
// ============================================================================
function renderVitestSection(data) {
  if (!data) return '';

  const lines = ['### 🧪 Vitest Unit Tests', '', '| Directory | Lines | Branches | Functions |', '|-----------|-------|----------|-----------|'];

  const calcPct = (stat) => (stat.total > 0 ? (stat.covered / stat.total) * 100 : 0);

  // Sort directories alphabetically
  const sortedDirs = [...data.byDirectory.entries()].sort((a, b) => a[0].localeCompare(b[0]));

  for (const [dir, stats] of sortedDirs) {
    const linesPct = calcPct(stats.lines);
    const branchesPct = calcPct(stats.branches);
    const funcsPct = calcPct(stats.functions);
    lines.push(`| ${dir} | ${formatPct(linesPct)} | ${formatPct(branchesPct)} | ${formatPct(funcsPct)} |`);
  }

  // Total row
  lines.push(
    `| **Total** | ${formatPct(getPct(data.total.lines), true)} | ${formatPct(getPct(data.total.branches), true)} | ${formatPct(getPct(data.total.functions), true)} |`
  );

  return lines.join('\n');
}

function renderPlaywrightSection(data) {
  if (!data) return '';

  const lines = ['### 🎭 Playwright E2E', '', '| Directory | Lines | Branches | Functions |', '|-----------|-------|----------|-----------|'];

  // Sort directories alphabetically
  const sortedDirs = [...data.byDirectory.entries()].sort((a, b) => a[0].localeCompare(b[0]));

  for (const [dir, stats] of sortedDirs) {
    const linesPct = getPct(stats.lines);
    const branchesPct = getPct(stats.branches);
    const funcsPct = getPct(stats.functions);
    lines.push(`| ${dir} | ${formatPct(linesPct)} | ${formatPct(branchesPct)} | ${formatPct(funcsPct)} |`);
  }

  // Total row
  lines.push(
    `| **Total** | ${formatPct(getPct(data.total.lines), true)} | ${formatPct(getPct(data.total.branches), true)} | ${formatPct(getPct(data.total.functions), true)} |`
  );

  return lines.join('\n');
}

function renderRustSection(rustData) {
  if (!rustData || rustData.length === 0) return '';

  const sections = ['### 🦀 Rust Backend'];

  for (const { label, data } of rustData) {
    if (!data) continue;

    const calcPct = (stat) => (stat.total > 0 ? (stat.covered / stat.total) * 100 : 0);

    sections.push('', `#### ${label}`, '', '| Module | Lines | Functions |', '|--------|-------|-----------|');

    // Sort modules alphabetically
    const sortedMods = [...data.byModule.entries()].sort((a, b) => a[0].localeCompare(b[0]));

    for (const [mod, stats] of sortedMods) {
      const linesPct = calcPct(stats.lines);
      const funcsPct = calcPct(stats.functions);
      sections.push(`| ${mod} | ${formatPct(linesPct)} | ${formatPct(funcsPct)} |`);
    }

    // Total row
    sections.push(`| **Total** | ${formatPct(getPct(data.total.lines), true)} | ${formatPct(getPct(data.total.functions), true)} |`);
  }

  return sections.join('\n');
}

// ============================================================================
// Main
// ============================================================================
function main() {
  const output = ['## 📊 Coverage Report', ''];
  let hasContent = false;

  // Vitest
  if (values.vitest) {
    const vitestData = parseVitestCoverage(values.vitest);
    const section = renderVitestSection(vitestData);
    if (section) {
      output.push(section, '');
      hasContent = true;
    }
  }

  // Playwright
  if (values.playwright) {
    const playwrightData = parsePlaywrightCoverage(values.playwright);
    const section = renderPlaywrightSection(playwrightData);
    if (section) {
      output.push(section, '');
      hasContent = true;
    }
  }

  // Rust (can have multiple files with labels)
  if (values.rust) {
    const rustFiles = values.rust.split(',').map((entry) => {
      const [path, label] = entry.includes(':') ? entry.split(':') : [entry, basename(entry, '.lcov')];
      return { path: path.trim(), label: label.trim() };
    });

    const rustData = rustFiles
      .map(({ path, label }) => ({
        label: label.charAt(0).toUpperCase() + label.slice(1) + ' Tests',
        data: parseLcovFile(path),
      }))
      .filter(({ data }) => data !== null);

    const section = renderRustSection(rustData);
    if (section) {
      output.push(section, '');
      hasContent = true;
    }
  }

  if (!hasContent) {
    output.push('*No coverage data available*');
  }

  console.log(output.join('\n'));
}

main();
