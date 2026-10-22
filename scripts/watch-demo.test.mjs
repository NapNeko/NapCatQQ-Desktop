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
  entryPoints: [path.join(webRoot, 'src/scripts/watch-demo.ts')],
  bundle: true, platform: 'browser', format: 'iife', write: false, target: 'es2022', logLevel: 'silent',
});

const fixture = `<section data-watch-demo data-watch-stage="0">
  <div class="watch-link"><span data-watch-link></span><div style="position:relative"><i class="watch-packet" style="position:absolute;top:-3px"></i></div></div>
  <div class="watch-notice-route" style="position:relative"><i style="position:absolute;left:-2px"></i></div>
  <div class="watch-scan"><i></i></div><div class="watch-radar"><i></i></div><div class="watch-notice"></div>
  <span data-watch-desktop></span><span data-watch-bot></span><span data-watch-notice-title></span><span data-watch-notice-copy></span>
  <button data-watch-play><span data-watch-play-label></span><span data-watch-play-icon></span></button>
  <div class="watch-meter" aria-valuenow="0"><i></i></div>
  <span data-watch-count></span><strong data-watch-title></strong><span data-watch-description></span>
  ${[0, 1, 2, 3].map(index => `<button data-watch-step="${index}">${index + 1}</button>`).join('')}
</section>`;

function setup(t) {
  const dom = new JSDOM(`<!doctype html><html><body>${fixture}</body></html>`, {
    url: 'https://docs.example.test/', runScripts: 'outside-only', pretendToBeVisual: true,
  });
  const { window } = dom;
  const { document } = window;
  const root = document.querySelector('[data-watch-demo]');
  const packet = root.querySelector('.watch-packet');
  const noticePacket = root.querySelector('.watch-notice-route > i');
  let width = 120;
  let height = 37;
  let reads = 0;
  const resizeCallbacks = [];
  const intersectionCallbacks = [];
  const motionListeners = new Set();
  const motion = { matches: false, addEventListener: (_, listener) => motionListeners.add(listener), removeEventListener: (_, listener) => motionListeners.delete(listener) };
  window.matchMedia = () => motion;
  window.requestAnimationFrame = () => 1;
  window.cancelAnimationFrame = () => {};
  window.ResizeObserver = class {
    constructor(callback) { resizeCallbacks.push(callback); }
    observe() {}
    disconnect() {}
  };
  window.IntersectionObserver = class {
    constructor(callback) { intersectionCallbacks.push(callback); }
    observe() {}
    disconnect() {}
  };
  Object.defineProperty(packet.parentElement, 'clientWidth', { get: () => { reads++; return width; } });
  Object.defineProperty(noticePacket.parentElement, 'clientHeight', { get: () => { reads++; return height; } });
  window.eval(compiled.outputFiles[0].text);
  t.after(() => {
    window.dispatchEvent(new window.PageTransitionEvent('pagehide', { persisted: false }));
    window.close();
  });
  const step = index => root.querySelector(`[data-watch-step="${index}"]`).click();
  const resize = (nextWidth, nextHeight) => { width = nextWidth; height = nextHeight; resizeCallbacks.forEach(callback => callback([])); };
  const intersect = visible => intersectionCallbacks.forEach(callback => callback([{ isIntersecting: visible, intersectionRatio: visible ? .7 : 0 }]));
  const translation = element => {
    const match = element.style.transform.match(/translate(?:3d)?\(([-\d.]+)(?:px)?,\s*([-\d.]+)(?:px)?/);
    return match ? [Number(match[1]), Number(match[2])] : [0, 0];
  };
  return { window, document, root, packet, noticePacket, step, resize, intersect, translation, reads: () => reads, motion, motionListeners };
}

test('值守步骤前进、后退和尺寸变化保持线路端点，播放位置不写入布局属性', t => {
  const page = setup(t);
  const reads = page.reads();
  page.step(3);
  assert.deepEqual(page.translation(page.packet), [120, 0]);
  assert.deepEqual(page.translation(page.noticePacket), [0, 37]);
  assert.equal(page.packet.style.left, '0px');
  assert.equal(page.noticePacket.style.top, '0px');
  assert.equal(page.reads(), reads, 'seeking must use cached path dimensions');

  page.resize(80, 52);
  assert.deepEqual(page.translation(page.packet), [80, 0]);
  assert.deepEqual(page.translation(page.noticePacket), [0, 52]);
  assert.equal(page.root.dataset.watchStage, '3');
  assert.equal(page.root.querySelector('.watch-meter').getAttribute('aria-valuenow'), '75');
  const resizedReads = page.reads();
  page.step(0);
  assert.deepEqual(page.translation(page.packet), [0, 0]);
  assert.deepEqual(page.translation(page.noticePacket), [0, 0]);
  assert.equal(page.reads(), resizedReads, 'backward seeking must not read layout');
});

test('值守进度只在整数百分比变化时更新，离屏与减少动态效果仍会暂停', t => {
  const page = setup(t);
  const meter = page.root.querySelector('.watch-meter');
  const setAttribute = meter.setAttribute.bind(meter);
  let progressWrites = 0;
  meter.setAttribute = (name, value) => { if (name === 'aria-valuenow') progressWrites++; setAttribute(name, value); };
  page.window.document.dispatchEvent(new page.window.Event('visibilitychange'));
  page.window.document.dispatchEvent(new page.window.Event('visibilitychange'));
  assert.equal(progressWrites, 0);
  page.step(1);
  assert.equal(meter.getAttribute('aria-valuenow'), '25');
  assert.equal(progressWrites, 1);
  page.step(1);
  assert.equal(progressWrites, 1);

  page.intersect(true);
  page.root.querySelector('[data-watch-play]').click();
  assert.equal(page.root.dataset.playing, 'true');
  page.intersect(false);
  assert.equal(page.root.dataset.playing, 'false');
  page.document.documentElement.dataset.theme = 'dark';
  page.intersect(true);
  assert.equal(page.root.dataset.playing, 'true');
  assert.equal(page.root.dataset.watchStage, '1');
  page.motion.matches = true;
  page.motionListeners.forEach(listener => listener());
  assert.equal(page.root.dataset.playing, 'false');
  assert.equal(page.root.querySelector('[data-watch-play-label]').textContent, '下一步');
});
