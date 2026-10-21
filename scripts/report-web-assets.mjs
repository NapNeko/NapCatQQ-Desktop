import { readFile, readdir } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { gzipSync } from 'node:zlib';

const siteRoot = fileURLToPath(new URL('../', import.meta.url));
const demoRoot = path.join(siteRoot, 'public/demo');
const manifestPath = path.join(demoRoot, '.vite/manifest.json');
const args = new Set(process.argv.slice(2));
for (const arg of args) {
  if (arg !== '--json') throw new Error(`Unknown option: ${arg}`);
}

let manifest;
try {
  manifest = JSON.parse(await readFile(manifestPath, 'utf8'));
} catch (error) {
  throw new Error('Demo build manifest is missing or invalid. Run pnpm run build:demo first.', { cause: error });
}
if (!manifest['index.html']?.isEntry) throw new Error('Demo manifest has no index.html entry.');
const entryHtml = await readFile(path.join(demoRoot, 'index.html'), 'utf8');
const entryStyles = [...entryHtml.matchAll(/<link\b[^>]*>/g)].flatMap(([tag]) => {
  if (!/\brel=["']stylesheet["']/.test(tag)) return [];
  const href = tag.match(/\bhref=["']([^"']+)["']/)?.[1];
  if (!href) throw new Error('Demo stylesheet link has no href.');
  return [href.replace(/^\.\//, '')];
});

const lessons = {
  'first-bot': ['lessons/first-bot.ts', 'modules/bot/BotPage.next.tsx'],
  connections: ['lessons/bots.ts', 'modules/bot/BotPage.next.tsx'],
  bots: ['lessons/bots.ts', 'modules/bot/BotPage.next.tsx'],
  'tasks-logs': ['lessons/bots.ts', 'modules/bot/BotPage.next.tsx'],
  components: ['lessons/system.ts', 'modules/components/ComponentsPage.next.tsx'],
  settings: ['lessons/system.ts', 'modules/settings/SettingsPage.next.tsx'],
  network: ['lessons/system.ts', 'modules/settings/SettingsPage.next.tsx'],
  backup: ['lessons/system.ts', 'modules/settings/SettingsPage.next.tsx'],
  appearance: ['lessons/system.ts', 'modules/settings/SettingsPage.next.tsx'],
  apps: ['lessons/workspaces.ts', 'modules/apps/AppsPage.next.tsx'],
  overview: ['lessons/workspaces.ts', 'modules/bootstrap/BootstrapPanel.next.tsx'],
  debugging: ['lessons/workspaces.ts', 'modules/debug/DebugConsolePage.tsx'],
  chat: ['lessons/workspaces.ts', 'modules/chat/ChatPage.tsx', 'chat.mock.ts'],
  terminal: ['lessons/terminal.ts', 'modules/bot/BotPage.next.tsx', 'modules/terminal/TerminalDock.tsx'],
};

function moduleKey(source) {
  const entries = Object.entries(manifest);
  const sourceOf = ([key, chunk]) => (chunk.src ?? key).replaceAll('\\', '/');
  const exact = entries.filter(entry => sourceOf(entry) === source);
  const matches = exact.length ? exact : entries.filter(entry => sourceOf(entry).endsWith(`/${source}`));
  if (matches.length !== 1) throw new Error(`Expected one manifest module for ${source}, found ${matches.length}.`);
  return matches[0][0];
}

function localAsset(file) {
  const absolute = path.resolve(demoRoot, file);
  const relative = path.relative(demoRoot, absolute);
  if (relative.startsWith('..') || path.isAbsolute(relative)) throw new Error(`Asset escapes the demo directory: ${file}`);
  return absolute;
}

function closure(roots) {
  const visited = new Set();
  const files = new Set(roots.includes('index.html') ? entryStyles : []);
  function visit(key) {
    if (visited.has(key)) return;
    const chunk = manifest[key];
    if (!chunk) throw new Error(`Manifest dependency is missing: ${key}`);
    visited.add(key);
    if (/\.(?:js|css)$/.test(chunk.file)) files.add(chunk.file);
    for (const css of chunk.css ?? []) files.add(css);
    for (const dependency of chunk.imports ?? []) visit(dependency);
  }
  for (const root of roots) visit(root);
  return [...files].sort();
}

async function listAssets(directory = demoRoot) {
  const entries = await readdir(directory, { withFileTypes: true });
  const groups = await Promise.all(entries.map(async entry => {
    const absolute = path.join(directory, entry.name);
    if (entry.isDirectory()) return listAssets(absolute);
    if (!entry.isFile() || !/\.(?:js|css)$/.test(entry.name)) return [];
    return [path.relative(demoRoot, absolute).replaceAll('\\', '/')];
  }));
  return groups.flat();
}

const allFiles = (await listAssets()).sort();
const sizes = new Map(await Promise.all(allFiles.map(async file => {
  const bytes = await readFile(localAsset(file));
  return [file, { rawBytes: bytes.length, gzipBytes: gzipSync(bytes).length }];
})));

function summarize(files) {
  const summary = {
    js: { files: 0, rawBytes: 0, gzipBytes: 0 },
    css: { files: 0, rawBytes: 0, gzipBytes: 0 },
  };
  for (const file of files) {
    const size = sizes.get(file);
    if (!size) throw new Error(`Built asset is missing: ${file}`);
    const group = file.endsWith('.css') ? summary.css : summary.js;
    group.files += 1;
    group.rawBytes += size.rawBytes;
    group.gzipBytes += size.gzipBytes;
  }
  return {
    ...summary,
    combined: {
      files: summary.js.files + summary.css.files,
      rawBytes: summary.js.rawBytes + summary.css.rawBytes,
      gzipBytes: summary.js.gzipBytes + summary.css.gzipBytes,
    },
    files,
  };
}

const report = {
  measurement: 'Static startup dependency closures; gzip is calculated locally, not measured network transfer.',
  total: summarize(allFiles),
  entry: summarize(closure(['index.html'])),
  lessons: Object.fromEntries(Object.entries(lessons).map(([id, sources]) => [
    id, summarize(closure(['index.html', ...sources.map(moduleKey)])),
  ])),
};

if (args.has('--json')) {
  console.log(JSON.stringify(report, null, 2));
} else {
  const size = bytes => `${(bytes / 1024).toFixed(1)} KiB`;
  console.log(report.measurement);
  console.table(Object.entries({ total: report.total, entry: report.entry, ...report.lessons }).map(([name, value]) => ({
    name,
    'JS files': value.js.files,
    'JS raw': size(value.js.rawBytes),
    'JS gzip': size(value.js.gzipBytes),
    'CSS raw': size(value.css.rawBytes),
    'CSS gzip': size(value.css.gzipBytes),
    'JS + CSS raw': size(value.combined.rawBytes),
    'JS + CSS gzip': size(value.combined.gzipBytes),
  })));
}
