import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const webRoot = fileURLToPath(new URL('../', import.meta.url));
const dependencyRoot = process.env.DESKTOP_DEMO_NODE_MODULES
  ? path.dirname(path.resolve(process.env.DESKTOP_DEMO_NODE_MODULES)) : path.join(webRoot, '.cache/desktop');
const desktopRequire = createRequire(path.join(dependencyRoot, 'package.json'));
const { build } = createRequire(desktopRequire.resolve('vite'))('esbuild');
const { JSDOM } = await import(pathToFileURL(desktopRequire.resolve('jsdom')).href);
const compiled = await build({
  entryPoints: [path.join(webRoot, 'src/scripts/layout-refresh.ts')],
  bundle: true, platform: 'browser', format: 'iife', globalName: 'layoutRefresh',
  write: false, target: 'es2022', logLevel: 'silent',
});

function setup(t) {
  const dom = new JSDOM('<main></main>', { runScripts: 'outside-only' });
  const { window } = dom;
  const tasks = new Map();
  let nextId = 0;
  let resize;
  let disconnected = false;
  let refreshes = 0;
  const target = window.document.querySelector('main');
  window.setTimeout = callback => { const id = ++nextId; tasks.set(id, callback); return id; };
  window.clearTimeout = id => tasks.delete(id);
  window.ResizeObserver = class {
    constructor(callback) { resize = callback; }
    observe(element) { assert.equal(element, target); }
    disconnect() { disconnected = true; }
  };
  const library = window.eval(`${compiled.outputFiles[0].text}\nlayoutRefresh;`);
  const observer = library.observeLayoutRefresh(target, () => refreshes++);
  t.after(() => { observer.dispose(); window.close(); });
  return {
    observer,
    size: (width, height) => resize([{ target, contentRect: { width, height } }]),
    flush() { const batch = [...tasks.values()]; tasks.clear(); batch.forEach(callback => callback()); },
    get pending() { return tasks.size; },
    get refreshes() { return refreshes; },
    get disconnected() { return disconnected; },
  };
}

test('font, load and resize bursts coalesce into one layout refresh', t => {
  const page = setup(t);
  page.observer.request();
  page.observer.request();
  for (let height = 900; height <= 1100; height += 10) page.size(1280, height);
  assert.equal(page.pending, 1);
  assert.equal(page.refreshes, 0);
  page.flush();
  assert.equal(page.refreshes, 1);
  page.size(1280, 1200);
  page.flush();
  assert.equal(page.refreshes, 2, 'a later content expansion still updates anchor positions');
});

test('identical and subpixel resize notifications do not reschedule a completed refresh', t => {
  const page = setup(t);
  page.size(1280, 900);
  page.flush();
  for (let i = 0; i < 100; i++) page.size(1280.25, 900.5);
  assert.equal(page.pending, 0);
  assert.equal(page.refreshes, 1);
  page.size(1280, 901.5);
  page.flush();
  assert.equal(page.refreshes, 2);
});

test('disposal cancels queued work and late font callbacks cannot restart it', t => {
  const page = setup(t);
  page.size(1280, 900);
  page.observer.dispose();
  assert.equal(page.disconnected, true);
  page.observer.request();
  page.size(1300, 950);
  page.flush();
  assert.equal(page.pending, 0);
  assert.equal(page.refreshes, 0);
});
