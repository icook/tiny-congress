#!/usr/bin/env node
// scripts/build-screenshot-gallery.mjs
//
// Generates an HTML gallery page from a directory of named screenshots.
// Groups images by page name (first segment of filename before viewport/theme).
//
// Usage: node scripts/build-screenshot-gallery.mjs --input dir --output path
// Optional: --sha <commit>, --branch <name>, --date <string>

import { readdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { parseArgs } from 'node:util';

const { values } = parseArgs({
  options: {
    input: { type: 'string' },
    output: { type: 'string' },
    sha: { type: 'string', default: '' },
    branch: { type: 'string', default: '' },
    date: { type: 'string', default: '' },
  },
});

if (!values.input || !values.output) {
  console.error('Usage: build-screenshot-gallery.mjs --input dir --output path');
  process.exit(1);
}

// Read and group screenshots
const files = readdirSync(values.input)
  .filter((f) => f.endsWith('.png'))
  .sort();

// Group by page name (e.g., "landing-desktop-dark.png" → "landing")
const groups = new Map();
for (const file of files) {
  const page = file.replace(/-(desktop|mobile)-(dark|light)\.png$/, '');
  if (!groups.has(page)) groups.set(page, []);
  groups.get(page).push(file);
}

const shortSha = values.sha?.slice(0, 7) || '';

// Generate gallery cards
const cards = [];
for (const [page, images] of groups) {
  const title = page
    .replace(/-/g, ' ')
    .replace(/\b\w/g, (c) => c.toUpperCase());
  const thumbs = images
    .map((img) => {
      const label = img
        .replace('.png', '')
        .replace(`${page}-`, '')
        .replace(/-/g, ' ');
      return `          <a href="${img}" class="thumb" target="_blank">
            <img src="${img}" alt="${img}" loading="lazy">
            <span class="label">${label}</span>
          </a>`;
    })
    .join('\n');

  cards.push(`        <div class="group">
          <h3>${title}</h3>
          <div class="thumbs">
${thumbs}
          </div>
        </div>`);
}

const html = `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Screenshots${shortSha ? ` — ${shortSha}` : ''}</title>
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
      max-width: 1200px; margin: 0 auto; padding: 24px;
      line-height: 1.6; background: var(--bg); color: var(--text);
    }
    .header {
      background: var(--bg-card); border-radius: 12px; padding: 24px;
      margin-bottom: 24px; box-shadow: var(--shadow); border: 1px solid var(--border);
      display: flex; align-items: center; justify-content: space-between;
    }
    .header h1 { margin: 0; font-size: 1.5rem; font-weight: 600; }
    .header .meta {
      color: var(--text-muted); font-size: 0.875rem;
      font-family: ui-monospace, 'SF Mono', monospace;
    }
    .header .meta a { color: var(--link); text-decoration: none; }
    .group {
      background: var(--bg-card); border-radius: 12px; padding: 20px;
      margin-bottom: 16px; box-shadow: var(--shadow); border: 1px solid var(--border);
    }
    .group h3 {
      margin: 0 0 16px 0; font-size: 1rem; font-weight: 600;
      color: var(--text-secondary);
    }
    .thumbs { display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 12px; }
    .thumb {
      display: flex; flex-direction: column; gap: 6px;
      padding: 8px; border-radius: 8px; border: 1px solid var(--border);
      background: var(--bg); text-decoration: none; color: var(--text);
      transition: border-color 0.15s;
    }
    .thumb:hover { border-color: var(--accent); }
    .thumb img {
      width: 100%; height: auto; border-radius: 4px;
      box-shadow: 0 1px 2px rgba(0,0,0,0.1);
    }
    .label {
      font-size: 0.75rem; color: var(--text-muted); text-transform: uppercase;
      letter-spacing: 0.05em; text-align: center;
    }
    .count {
      background: var(--bg-hover); color: var(--text-muted); padding: 2px 8px;
      border-radius: 10px; font-size: 0.75rem; margin-left: 8px;
    }
  </style>
</head>
<body>
  <header class="header">
    <div>
      <h1>Screenshots<span class="count">${files.length}</span></h1>
    </div>
    <div class="meta">
      ${shortSha ? `<a href="https://github.com/icook/tiny-congress/commit/${values.sha}">${shortSha}</a>` : ''}
      ${values.branch ? ` &middot; ${values.branch}` : ''}
      ${values.date ? ` &middot; ${values.date}` : ''}
    </div>
  </header>
${cards.join('\n')}
</body>
</html>
`;

writeFileSync(values.output, html);
console.log(
  `Gallery: ${files.length} screenshots in ${groups.size} groups → ${values.output}`
);
