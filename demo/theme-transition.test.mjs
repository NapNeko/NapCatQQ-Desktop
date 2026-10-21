import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = fileURLToPath(new URL('./', import.meta.url));
const dependencies = process.env.DESKTOP_DEMO_NODE_MODULES
  ? path.dirname(path.resolve(process.env.DESKTOP_DEMO_NODE_MODULES)) : path.resolve(root, '../.cache/desktop');
const desktopRequire = createRequire(path.join(dependencies, 'package.json'));
const { build } = createRequire(desktopRequire.resolve('vite'))('esbuild');
const { JSDOM } = await import(pathToFileURL(desktopRequire.resolve('jsdom')).href);
const compiled = await build({
  entryPoints: [path.join(root, 'theme-transition.ts')], bundle: true, platform: 'browser',
  format: 'iife', globalName: 'ThemeTest', target: 'es2022', write: false, logLevel: 'silent',
});
const options = { enabled: true, level: 'rich', duration: 1.4, easing: 'ease-in-out' };

function fixture(t, { hidden = false, reduced = false } = {}) {
  const dom = new JSDOM('<!doctype html><div class="demo-desktop">设置</div>', {
    runScripts: 'outside-only', pretendToBeVisual: true,
  });
  t.after(() => dom.window.close());
  const { window } = dom;
  const document = window.document;
  Object.defineProperty(document, 'hidden', { get: () => hidden });
  window.matchMedia = () => ({ matches: reduced });
  document.startViewTransition = () => { assert.fail('iframe 不允许创建整页快照'); };
  const animations = [];
  document.querySelector('.demo-desktop').animate = () => {
    let reject;
    const animation = { cancelled: false, finished: new Promise((_, fail) => { reject = fail; }),
      cancel() { this.cancelled = true; reject(new Error('Animation cancelled')); } };
    animations.push(animation);
    return animation;
  };
  const api = window.eval(`${compiled.outputFiles[0].text}\nThemeTest;`);
  return { ...api, document, window, animations };
}

test('iframe 主题立即应用，不依赖整页快照或局部动画完成', async t => {
  const f = fixture(t);
  const change = theme => f.playThemeTransition(() => { f.document.documentElement.dataset.theme = theme; }, options);
  await change('dark');
  assert.equal(f.document.documentElement.dataset.theme, 'dark');
  await change('light');
  assert.equal(f.document.documentElement.dataset.theme, 'light');
  assert.equal(f.animations[0].cancelled, true);
  assert.equal(f.document.body.children.length, 1);
  assert.equal(f.document.querySelector('.demo-desktop').style.opacity, '');
  f.window.dispatchEvent(new f.window.Event('pagehide'));
  assert.equal(f.animations[1].cancelled, true);
});

test('后台、减少动画和动画不可用时仍完成主题保存', async t => {
  for (const config of [{ hidden: true }, { reduced: true }, {}]) {
    const f = fixture(t, config);
    if (!config.hidden && !config.reduced) {
      f.document.querySelector('.demo-desktop').animate = () => { throw new Error('Unavailable'); };
    }
    await f.playThemeTransition(() => { f.document.documentElement.dataset.theme = 'dark'; }, options);
    assert.equal(f.document.documentElement.dataset.theme, 'dark');
    assert.equal(f.animations.length, 0);
  }
});
