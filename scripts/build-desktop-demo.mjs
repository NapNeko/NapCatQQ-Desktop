import { spawn, execFileSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { promises as fs } from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const siteRoot = fileURLToPath(new URL('../', import.meta.url));
const cacheRoot = path.join(siteRoot, '.cache', 'desktop');
const source = JSON.parse(await fs.readFile(path.join(siteRoot, 'demo/source.json'), 'utf8'));
const expectedPaths = ['src-ui', 'package.json', 'pnpm-lock.yaml'];
const flags = new Set(process.argv.slice(2));
for (const flag of flags) {
  if (!['--dry-run', '--prepare', '--fetch-source'].includes(flag)) throw new Error(`Unknown option: ${flag}`);
}
if (!/^[a-f0-9]{40}$/.test(source.commit) || JSON.stringify(source.include) !== JSON.stringify(expectedPaths)) {
  throw new Error('demo/source.json must pin a full Git commit and the reviewed frontend-only allowlist.');
}

function git(args, encoding = 'utf8') {
  return execFileSync('git', args, { cwd: siteRoot, encoding, maxBuffer: 16 * 1024 * 1024, stdio: ['ignore', 'pipe', 'pipe'] });
}

function waitFor(child, label) {
  return new Promise((resolve, reject) => {
    child.once('error', reject);
    child.once('close', (code, signal) => code === 0 ? resolve() : reject(new Error(`${label} failed (${signal || code}).`)));
  });
}

async function run(command, args, cwd = siteRoot) {
  await waitFor(spawn(command, args, { cwd, stdio: 'inherit', windowsHide: true }), command);
}

function hasSourceObject() {
  try { git(['cat-file', '-e', `${source.commit}^{commit}`]); return true; }
  catch { return false; }
}

if (!hasSourceObject()) {
  if (flags.has('--dry-run') && !flags.has('--fetch-source')) {
    throw new Error('Pinned source object is missing. Run demo:plan -- --fetch-source to fetch that object before reviewing the export plan.');
  }
  console.log(`Fetching pinned source object ${source.commit}; no branch checkout or merge.`);
  await run('git', ['fetch', '--no-tags', '--no-write-fetch-head', source.repository, source.commit]);
}

const entries = git(['ls-tree', '-r', '-z', source.commit, '--', ...expectedPaths])
  .split('\0').filter(Boolean).map(line => {
    const tab = line.indexOf('\t');
    const [mode, type, hash] = line.slice(0, tab).split(' ');
    const name = line.slice(tab + 1);
    if (!['100644', '100755'].includes(mode) || type !== 'blob'
      || path.posix.isAbsolute(name) || name.split('/').includes('..')
      || !(name.startsWith('src-ui/') || expectedPaths.slice(1).includes(name))
      || name.startsWith('src-ui/docs/')) {
      throw new Error(`Export rejected unexpected source entry: ${name}`);
    }
    return { name, hash };
  });
for (const name of ['package.json', 'pnpm-lock.yaml', 'src-ui/app/index.css']) {
  if (!entries.some(entry => entry.name === name)) throw new Error(`Pinned source is missing ${name}.`);
}
const lockHash = entries.find(entry => entry.name === 'pnpm-lock.yaml').hash;
const manifest = { commit: source.commit, include: expectedPaths, fileCount: entries.length, lockHash };
console.log(`Desktop demo source: v${source.version} / ${source.commit}`);
console.log(`Export destination: ${cacheRoot}`);
for (const name of expectedPaths) {
  console.log(`  ${name}: ${entries.filter(entry => entry.name === name || entry.name.startsWith(`${name}/`)).length} files`);
}
console.log(`Total: ${entries.length} tracked files. No crates, src-tauri, main docs, configuration, or ignored files are exported.`);
console.log(`Generated output: ${path.join(siteRoot, 'public/demo')} (base ./)`);
if (flags.has('--dry-run')) process.exit(0);

async function exists(name) {
  try { await fs.lstat(name); return true; }
  catch (error) { if (error.code === 'ENOENT') return false; throw error; }
}

async function prepareSource() {
  const marker = path.join(cacheRoot, '.desktop-source.json');
  if (await exists(cacheRoot)) {
    let previous;
    try { previous = JSON.parse(await fs.readFile(marker, 'utf8')); }
    catch { throw new Error(`${cacheRoot} already exists without valid provenance; move it aside before rebuilding.`); }
    if (JSON.stringify(previous) !== JSON.stringify(manifest)) {
      throw new Error('Cached source differs from the pinned export. Move .cache/desktop aside and review demo:plan before rebuilding.');
    }
    console.log('Using the existing pinned source export.');
    return;
  }
  const cacheParent = path.dirname(cacheRoot);
  await fs.mkdir(cacheParent, { recursive: true });
  await fs.mkdir(cacheRoot);
  // GNU tar may close stdin at the archive footer before Node finishes piping its padding.
  const archive = path.join(cacheRoot, '.desktop-source.tar');
  await run('git', ['archive', '--format=tar', `--output=${archive}`, source.commit, ...expectedPaths]);
  await run('tar', ['-xf', '.desktop-source.tar'], cacheRoot);
  await fs.unlink(archive);
  for (const entry of entries) {
    if (!(await fs.lstat(path.join(cacheRoot, entry.name))).isFile()) {
      throw new Error(`Export is incomplete: ${entry.name}`);
    }
  }
  // Windows can keep an extracted directory handle open briefly; a fixed destination avoids a rename race.
  await fs.writeFile(marker, `${JSON.stringify(manifest, null, 2)}\n`);
  console.log('Pinned frontend export prepared.');
}

function dependencyRoot() {
  return process.env.DESKTOP_DEMO_NODE_MODULES
    ? path.dirname(path.resolve(process.env.DESKTOP_DEMO_NODE_MODULES)) : cacheRoot;
}

function desktopRequire() {
  return createRequire(path.join(dependencyRoot(), 'package.json'));
}

async function loadDesktopEsm(name) {
  const packageRoot = path.join(dependencyRoot(), 'node_modules', name);
  const metadata = JSON.parse(await fs.readFile(path.join(packageRoot, 'package.json'), 'utf8'));
  const rootExport = metadata.exports?.['.'] ?? metadata.exports;
  const imported = typeof rootExport === 'string' ? rootExport : rootExport?.import ?? rootExport?.default;
  const entry = typeof imported === 'string' ? imported : imported?.default;
  if (typeof entry !== 'string' || !entry.startsWith('./')) {
    throw new Error(`No supported ESM runtime export for ${name}@${metadata.version}.`);
  }
  return import(pathToFileURL(path.resolve(packageRoot, entry)).href);
}

async function prepareDependencies() {
  const nodeModules = path.join(cacheRoot, 'node_modules');
  const reuse = process.env.DESKTOP_DEMO_NODE_MODULES;
  if (reuse) {
    const target = await fs.realpath(path.resolve(reuse));
    if (!await exists(path.join(target, 'vite/package.json'))) throw new Error('DESKTOP_DEMO_NODE_MODULES must point to an installed node_modules containing Vite.');
    console.log('Resolving explicitly selected local dependencies read-only; no directory links are created.');
    return;
  }
  if (await exists(nodeModules) && (await fs.lstat(nodeModules)).isSymbolicLink()) {
    throw new Error('Cache contains a local dependency link. Set DESKTOP_DEMO_NODE_MODULES to reuse it, or move the generated cache aside for an independent installation.');
  }
  const dependencyMarker = path.join(cacheRoot, '.desktop-dependencies.json');
  if (await exists(dependencyMarker)) {
    const previous = JSON.parse(await fs.readFile(dependencyMarker, 'utf8'));
    try {
      if (previous.lockHash === lockHash && desktopRequire().resolve('vite')) return;
    } catch { /* Reinstall incomplete generated dependencies. */ }
  }
  const args = ['install', '--frozen-lockfile', '--ignore-scripts', '--ignore-workspace'];
  const pnpmEntry = process.env.npm_execpath;
  if (pnpmEntry && /pnpm/i.test(path.basename(pnpmEntry))) {
    if (/\.[cm]?js$/i.test(pnpmEntry)) await run(process.execPath, [pnpmEntry, ...args], cacheRoot);
    else if (process.platform !== 'win32' || /\.exe$/i.test(pnpmEntry)) await run(pnpmEntry, args, cacheRoot);
    else throw new Error('The pnpm entry must be a JavaScript CLI or standalone executable, not a Windows shell shim.');
  } else if (process.platform !== 'win32') {
    await run('pnpm', args, cacheRoot);
  } else {
    throw new Error('Run this command through pnpm run demo:prepare or pnpm run build:demo so Windows can execute the pnpm CLI without a shell shim.');
  }
  await fs.writeFile(dependencyMarker, `${JSON.stringify({ lockHash })}\n`);
}

await prepareSource();
await prepareDependencies();
if (flags.has('--prepare')) process.exit(0);
const vite = await loadDesktopEsm('vite');
if (typeof vite.build !== 'function') throw new Error('The pinned Vite ESM entry does not export build().');
await vite.build({ configFile: path.join(siteRoot, 'demo/vite.config.mjs') });
