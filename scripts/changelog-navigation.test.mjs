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
  entryPoints: [path.join(webRoot, 'src/scripts/changelog.ts')],
  bundle: true, platform: 'browser', format: 'iife', write: false, target: 'es2022', logLevel: 'silent',
});

const entries = [
  { tag: 'v3.5.2', product: 'desktop', latest: true },
  { tag: 'v3.6.0-beta.1', product: 'desktop', prerelease: true },
  { tag: 'v3.5.1', product: 'desktop' },
  { tag: 'watch-v0.3.0', product: 'watch', latest: true },
  { tag: 'watch-v0.2.9', product: 'watch' },
  { tag: 'watch-v0.4.0-rc.1', product: 'watch', prerelease: true },
];
const fixtureHtml = `
  <button data-changelog-theme aria-pressed="false">切换主题</button>
  <button data-changelog-menu aria-expanded="false" aria-controls="navigation">菜单</button>
  <nav id="navigation" data-changelog-nav><a href="/NapCatQQ-Desktop/docs/">使用文档</a></nav>
  <main data-changelog>
    <div data-changelog-controls>
      <div role="tablist" aria-label="产品">
        <button role="tab" data-product="desktop">Desktop</button>
        <button role="tab" data-product="watch">ncd-watch</button>
      </div>
      <label><input type="checkbox" data-preview-toggle>显示预览版本</label>
      <select data-release-select aria-label="跳转版本">${entries.map(entry => `<option value="#${entry.tag}">${entry.tag}</option>`).join('')}</select>
    </div>
    <p data-product-panel="desktop">桌面客户端</p><p data-product-panel="watch">远端值守</p>
    <nav data-changelog-index aria-label="版本索引"><ul>${entries.map(entry => `<li data-release-index><a data-release-link href="#${entry.tag}">${entry.tag}</a></li>`).join('')}</ul></nav>
    <div data-release-timeline>${entries.map(entry => `
      <article id="${entry.tag}" data-release data-release-tag="${entry.tag}" data-product="${entry.product}" data-prerelease="${!!entry.prerelease}" data-release-latest="${!!entry.latest}">
        <h2>${entry.tag}</h2>
        ${entry.latest ? '<p data-release-body>原始发版说明</p>' : '<details data-release-details open><summary>查看更新内容</summary><p data-release-body>原始发版说明</p></details>'}
        <button data-copy-release="${entry.tag}"><span data-copy-label>复制链接</span></button>
      </article>`).join('')}</div>
    <p role="status" data-changelog-status></p>
  </main>`;

function setup(t, { hash = '', clipboardFailure = false } = {}) {
  const dom = new JSDOM(`<!doctype html><html data-theme="light"><body>${fixtureHtml}</body></html>`, {
    url: `https://docs.example.test/NapCatQQ-Desktop/changelog/?from=docs${hash}`,
    runScripts: 'outside-only', pretendToBeVisual: true,
  });
  const { window } = dom;
  const { document } = window;
  const root = document.querySelector('[data-changelog]');
  const clipboard = [];
  const errors = [];
  const scrolls = [];
  const pendingFrames = new Map();
  let nextFrame = 0;
  let scrollOffset = 0;
  window.matchMedia = () => ({ matches: true, addEventListener() {}, removeEventListener() {} });
  window.requestAnimationFrame = callback => { const id = ++nextFrame; pendingFrames.set(id, callback); return id; };
  window.cancelAnimationFrame = id => pendingFrames.delete(id);
  Object.defineProperty(window.navigator, 'clipboard', { value: {
    async writeText(value) {
      if (clipboardFailure) throw new Error('Clipboard permission denied');
      clipboard.push(value);
    },
  } });
  document.startViewTransition = () => { throw new Error('Theme changes must not snapshot the page'); };
  window.addEventListener('error', event => errors.push(event.error));
  const originalRect = window.HTMLElement.prototype.getBoundingClientRect;
  const bounds = (top, height = 300) => ({ x: 0, y: top, top, bottom: top + height, left: 0, right: 800, width: 800, height, toJSON() { return this; } });
  const visibleReleases = () => Array.from(root.querySelectorAll('[data-release]')).filter(release => !release.hidden);
  window.HTMLElement.prototype.getBoundingClientRect = function () {
    if (this.hasAttribute('data-release')) return this.hidden ? bounds(0, 0) : bounds(visibleReleases().indexOf(this) * 600 + 250 - scrollOffset);
    if (this.hasAttribute('data-changelog-controls')) return bounds(72, 60);
    if (this.hasAttribute('data-release-timeline')) return bounds(250 - scrollOffset, visibleReleases().length * 600);
    return originalRect.call(this);
  };
  window.HTMLElement.prototype.scrollIntoView = function (options) {
    scrolls.push({ id: this.id, options });
    if (this.hasAttribute('data-release')) {
      scrollOffset = visibleReleases().indexOf(this) * 600 + 250 - 140;
      window.dispatchEvent(new window.Event('scroll'));
    }
  };
  function flushFrames() {
    for (let turn = 0; pendingFrames.size && turn < 10; turn++) {
      const batch = [...pendingFrames.values()];
      pendingFrames.clear();
      batch.forEach(callback => callback(turn * 16));
    }
    assert.equal(pendingFrames.size, 0, 'animation frame loop must settle');
  }
  window.eval(compiled.outputFiles[0].text);
  flushFrames();
  t.after(() => {
    assert.deepEqual(errors, [], 'controller must not raise window errors');
    window.close();
  });
  const getRelease = tag => document.getElementById(tag);
  const visibleTags = () => Array.from(root.querySelectorAll('[data-release]')).filter(release => !release.hidden).map(release => release.id);
  const visibleLinks = () => Array.from(root.querySelectorAll('[data-release-link]')).filter(link => !link.closest('[data-release-index]').hidden).map(link => link.hash.slice(1));
  const enabledOptions = () => Array.from(root.querySelector('[data-release-select]').options).filter(option => !option.disabled).map(option => option.value.slice(1));
  const productButton = product => root.querySelector(`button[data-product="${product}"]`);
  async function settle() {
    await Promise.resolve();
    await Promise.resolve();
    flushFrames();
  }
  return { window, document, root, clipboard, scrolls, flushFrames, settle, getRelease, visibleTags, visibleLinks, enabledOptions, productButton };
}

test('默认只展示 Desktop 正式版，产品和预览筛选同步正文、版本索引与手机选项', t => {
  const page = setup(t);
  const assertViews = expected => {
    assert.deepEqual(page.visibleTags(), expected);
    assert.deepEqual(page.visibleLinks(), expected);
    assert.deepEqual(page.enabledOptions(), expected);
  };
  assertViews(['v3.5.2', 'v3.5.1']);
  assert.equal(page.root.querySelector('[data-product-panel="watch"]').hidden, true);
  page.productButton('watch').click();
  page.flushFrames();
  assertViews(['watch-v0.3.0', 'watch-v0.2.9']);
  assert.equal(page.window.location.hash, '#watch-v0.3.0');
  assert.equal(page.root.querySelector('[data-product-panel="desktop"]').hidden, true);

  const preview = page.root.querySelector('[data-preview-toggle]');
  preview.checked = true;
  preview.dispatchEvent(new page.window.Event('change'));
  page.flushFrames();
  assertViews(['watch-v0.3.0', 'watch-v0.2.9', 'watch-v0.4.0-rc.1']);
  page.productButton('desktop').click();
  page.flushFrames();
  assertViews(['v3.5.2', 'v3.6.0-beta.1', 'v3.5.1']);
});

test('打开 ncd-watch 历史版本深链时切换产品并展开正文，减少动画模式直接定位', t => {
  const page = setup(t, { hash: '#watch-v0.2.9' });
  assert.equal(page.productButton('watch').getAttribute('aria-selected'), 'true');
  assert.deepEqual(page.visibleTags(), ['watch-v0.3.0', 'watch-v0.2.9']);
  assert.equal(page.getRelease('watch-v0.2.9').querySelector('details').open, true);
  assert.equal(page.getRelease('v3.5.1').querySelector('details').open, false);
  assert.equal(page.root.querySelector('[data-release-select]').value, '#watch-v0.2.9');
  assert.equal(page.root.querySelector('[aria-current="location"]').hash, '#watch-v0.2.9');
  assert.equal(page.scrolls.at(-1).id, 'watch-v0.2.9');
  assert.equal(page.scrolls.at(-1).options.behavior, 'instant');
});

test('在历史版本深处切换产品，首版回到控制栏下方且索引与 URL 一致', t => {
  const page = setup(t);
  page.root.querySelector('[data-release-link][href="#v3.5.1"]').click();
  page.flushFrames();
  const controls = page.root.querySelector('[data-changelog-controls]');
  assert.ok(page.getRelease('v3.5.2').getBoundingClientRect().top < controls.getBoundingClientRect().bottom);

  page.productButton('watch').click();
  page.flushFrames();
  const first = page.getRelease('watch-v0.3.0');
  const top = first.getBoundingClientRect().top;
  const controlsBottom = controls.getBoundingClientRect().bottom;
  assert.equal(page.scrolls.at(-1).id, first.id);
  assert.equal(page.scrolls.at(-1).options.behavior, 'instant');
  assert.ok(top >= controlsBottom && top < controlsBottom + 80, 'new product starts visibly below the sticky controls');
  assert.equal(page.window.location.hash, '#watch-v0.3.0');
  assert.equal(page.root.querySelector('[aria-current="location"]').hash, '#watch-v0.3.0');
  assert.equal(page.root.querySelector('[data-release-select]').value, '#watch-v0.3.0');
});

test('预览版深链自动开启预览筛选，关闭后回到可见的正式版本', t => {
  const page = setup(t, { hash: '#v3.6.0-beta.1' });
  const preview = page.root.querySelector('[data-preview-toggle]');
  assert.equal(preview.checked, true);
  assert.equal(page.getRelease('v3.6.0-beta.1').hidden, false);
  assert.equal(page.getRelease('v3.6.0-beta.1').querySelector('details').open, true);
  preview.checked = false;
  preview.dispatchEvent(new page.window.Event('change'));
  page.flushFrames();
  assert.equal(page.getRelease('v3.6.0-beta.1').hidden, true);
  assert.equal(page.window.location.hash, '#v3.5.2');
  assert.equal(page.root.querySelector('[data-release-select]').value, '#v3.5.2');
  assert.equal(page.scrolls.at(-1).id, 'v3.5.2');
  assert.ok(page.getRelease('v3.5.2').getBoundingClientRect().top >= page.root.querySelector('[data-changelog-controls]').getBoundingClientRect().bottom);
});

test('无效或未知版本 hash 保持可读的默认列表', t => {
  for (const hash of ['#%E0%A4%A', '#not-a-release']) {
    const page = setup(t, { hash });
    assert.deepEqual(page.visibleTags(), ['v3.5.2', 'v3.5.1']);
    assert.equal(page.root.querySelector('[data-release-select]').value, '#v3.5.2');
  }
});

test('产品标签支持方向键和首尾键，手机选项和索引链接都能打开对应版本', t => {
  const page = setup(t);
  page.productButton('desktop').focus();
  page.productButton('desktop').dispatchEvent(new page.window.KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true, cancelable: true }));
  page.flushFrames();
  assert.equal(page.document.activeElement, page.productButton('watch'));
  assert.equal(page.productButton('watch').tabIndex, 0);
  assert.equal(page.productButton('desktop').tabIndex, -1);

  page.productButton('watch').dispatchEvent(new page.window.KeyboardEvent('keydown', { key: 'Home', bubbles: true, cancelable: true }));
  page.flushFrames();
  assert.equal(page.document.activeElement, page.productButton('desktop'));
  const picker = page.root.querySelector('[data-release-select]');
  picker.value = '#v3.5.1';
  picker.dispatchEvent(new page.window.Event('change'));
  page.flushFrames();
  assert.equal(page.window.location.hash, '#v3.5.1');
  assert.equal(page.getRelease('v3.5.1').querySelector('details').open, true);
  assert.equal(page.root.querySelector('[aria-current="location"]').hash, '#v3.5.1');

  page.productButton('desktop').dispatchEvent(new page.window.KeyboardEvent('keydown', { key: 'End', bubbles: true, cancelable: true }));
  page.flushFrames();
  page.root.querySelector('[data-release-link][href="#watch-v0.2.9"]').click();
  page.flushFrames();
  assert.equal(page.window.location.hash, '#watch-v0.2.9');
  assert.equal(page.getRelease('watch-v0.2.9').querySelector('details').open, true);
  assert.equal(page.root.querySelector('[data-release-select]').value, '#watch-v0.2.9');
});

test('复制链接保留站点子路径与查询参数，剪贴板拒绝时提供地址栏退路', async t => {
  const page = setup(t);
  page.root.querySelector('[data-release-link][href="#v3.5.1"]').click();
  page.flushFrames();
  const copy = page.getRelease('v3.5.1').querySelector('[data-copy-release]');
  copy.click();
  await page.settle();
  assert.deepEqual(page.clipboard, ['https://docs.example.test/NapCatQQ-Desktop/changelog/?from=docs#v3.5.1']);
  assert.equal(copy.querySelector('[data-copy-label]').textContent, '已复制');
  assert.equal(page.root.querySelector('[data-changelog-status]').textContent, 'v3.5.1 的链接已复制');

  const fallback = setup(t, { clipboardFailure: true });
  fallback.root.querySelector('[data-copy-release="v3.5.2"]').click();
  await fallback.settle();
  assert.equal(fallback.window.location.hash, '#v3.5.2');
  assert.equal(fallback.root.querySelector('[data-changelog-status]').textContent, '可以从地址栏复制此版本链接');
  assert.deepEqual(fallback.clipboard, []);
});

test('主题切换持久化并保留当前版本和正文节点，跨页主题同步不调用整页快照', t => {
  const page = setup(t, { hash: '#watch-v0.2.9' });
  const body = page.getRelease('watch-v0.2.9').querySelector('[data-release-body]');
  const button = page.document.querySelector('[data-changelog-theme]');
  button.click();
  assert.equal(page.document.documentElement.dataset.theme, 'dark');
  assert.equal(page.window.localStorage.getItem('ncd-docs-theme'), 'dark');
  assert.equal(button.getAttribute('aria-pressed'), 'true');
  assert.equal(button.getAttribute('aria-label'), '切换到浅色主题');
  assert.equal(page.getRelease('watch-v0.2.9').querySelector('[data-release-body]'), body);
  assert.equal(page.getRelease('watch-v0.2.9').querySelector('details').open, true);
  assert.equal(page.window.location.hash, '#watch-v0.2.9');

  page.window.dispatchEvent(new page.window.StorageEvent('storage', { key: 'ncd-docs-theme', newValue: 'light' }));
  assert.equal(page.document.documentElement.dataset.theme, 'light');
  assert.equal(button.getAttribute('aria-pressed'), 'false');
  page.window.dispatchEvent(new page.window.StorageEvent('storage', { key: 'ncd-docs-theme', newValue: 'invalid-theme' }));
  assert.equal(page.document.documentElement.dataset.theme, 'light');
  assert.equal(page.getRelease('watch-v0.2.9').querySelector('[data-release-body]'), body);
});
