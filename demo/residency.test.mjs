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
const options = { bundle: true, platform: 'browser', format: 'iife', write: false, logLevel: 'silent', target: 'es2022' };
const [policyBuild, controllerBuild] = await Promise.all([
  build({ ...options, entryPoints: [path.join(webRoot, 'src/scripts/demo-residency.ts')], globalName: 'ResidencyTest' }),
  build({ ...options, entryPoints: [path.join(webRoot, 'src/scripts/tutorial-demo.ts')] }),
]);
const { DemoResidency } = Function(`${policyBuild.outputFiles[0].text}; return ResidencyTest;`)();

function fakeClock() {
  let now = 0;
  let next = 0;
  const timers = new Map();
  return {
    now: () => now,
    setTimeout(callback, delay) { const id = ++next; timers.set(id, { at: now + delay, callback }); return id; },
    clearTimeout(id) { timers.delete(id); },
    advance(ms) {
      const end = now + ms;
      for (;;) {
        const entry = [...timers].filter(([, timer]) => timer.at <= end).sort((a, b) => a[1].at - b[1].at)[0];
        if (!entry) break;
        const [id, timer] = entry;
        now = timer.at;
        timers.delete(id);
        timer.callback();
      }
      now = end;
    },
  };
}

test('快速来回滚动保留会话，连续离屏 12 秒后释放', () => {
  const clock = fakeClock();
  const pool = new DemoResidency(clock);
  let evictions = 0;
  const session = pool.register(() => { evictions += 1; });
  session.update({ visible: true, protected: false });
  assert.equal(session.acquire(), true);
  session.update({ visible: false, protected: false });
  clock.advance(11_000);
  assert.equal(evictions, 0);
  session.update({ visible: true, protected: false });
  clock.advance(20_000);
  assert.equal(evictions, 0);
  session.update({ visible: false, protected: false });
  clock.advance(12_000);
  assert.equal(evictions, 1);
});

test('第三个自动演示优先回收最早离屏的窗口，不驱逐正在看的窗口', () => {
  const clock = fakeClock();
  const pool = new DemoResidency(clock);
  const evicted = [];
  const sessions = ['first', 'second', 'third', 'fourth'].map(id => pool.register(() => evicted.push(id)));
  sessions[0].update({ visible: true, protected: false });
  sessions[0].acquire();
  sessions[0].update({ visible: false, protected: false });
  clock.advance(100);
  sessions[1].update({ visible: true, protected: false });
  sessions[1].acquire();
  sessions[2].update({ visible: true, protected: false });
  assert.equal(sessions[2].acquire(), true);
  assert.deepEqual(evicted, ['first']);
  sessions[3].update({ visible: true, protected: false });
  assert.equal(sessions[3].acquire(), false);
  assert.deepEqual(evicted, ['first']);
});

test('手动或展开会话不参与自动预算，解除保留后重新计算离屏宽限期', () => {
  const clock = fakeClock();
  const pool = new DemoResidency(clock);
  const evicted = [];
  const manual = pool.register(() => evicted.push('manual'));
  manual.update({ visible: false, protected: true });
  manual.acquire();
  const automatic = pool.register(() => evicted.push('automatic'));
  automatic.update({ visible: true, protected: false });
  automatic.acquire();
  clock.advance(60_000);
  assert.deepEqual(evicted, []);
  manual.update({ visible: false, protected: false });
  clock.advance(11_999);
  assert.deepEqual(evicted, []);
  clock.advance(1);
  assert.deepEqual(evicted, ['manual']);
});

test('窗口离屏或改为手动保留后，立即通知等待预算的演示', () => {
  const clock = fakeClock();
  let notifications = 0;
  const pool = new DemoResidency(clock, 2, 12_000, () => { notifications += 1; });
  const session = pool.register(() => {});
  session.update({ visible: true, protected: false });
  session.acquire();
  session.update({ visible: false, protected: false });
  assert.equal(notifications, 1);
  session.update({ visible: false, protected: false });
  assert.equal(notifications, 1);
  session.update({ visible: true, protected: true });
  assert.equal(notifications, 2);
});

const fixtureHtml = `
<div data-tutorial-anchor><section data-tutorial-demo data-demo-showcase data-demo-lesson="chat" data-demo-src="/demo/index.html?lesson=chat">
  <div class="fbd-toolbar">
    <button data-fbd-play><svg data-fbd-play-icon></svg><svg data-fbd-pause-icon></svg><span data-fbd-play-label></span></button>
    <button data-fbd-manual></button><button data-fbd-reset></button>
    <button data-fbd-expand><span data-fbd-expand-label></span></button>
    <details data-fbd-progress><summary></summary><div class="fbd-step-panel"><ol class="fbd-steps"><li data-fbd-step="0"></li><li data-fbd-step="1"></li></ol></div></details>
    <span data-fbd-mode></span><span data-fbd-position></span><span data-fbd-title></span><span data-fbd-detail></span>
  </div>
  <div data-fbd-viewport><div data-fbd-cover></div><div data-fbd-loading></div><iframe data-fbd-frame width="1280" height="720"></iframe></div>
  <div data-fbd-error hidden></div><div data-fbd-pan></div>
</section></div>`;

function controller(t, { compact = false, reduced = false, count = 1, showcase = true } = {}) {
  const fixture = showcase ? fixtureHtml : fixtureHtml.replace(' data-demo-showcase', '');
  const dom = new JSDOM(`<!doctype html>${fixture.repeat(count)}`, {
    url: 'https://docs.example.test/', runScripts: 'outside-only', pretendToBeVisual: true,
  });
  const { window } = dom;
  let focused = true;
  window.document.hasFocus = () => focused;
  const clock = fakeClock();
  window.setTimeout = clock.setTimeout;
  window.clearTimeout = clock.clearTimeout;
  Object.defineProperty(window.performance, 'now', { value: clock.now });
  window.requestAnimationFrame = callback => clock.setTimeout(callback, 16);
  const queries = new Map();
  window.matchMedia = query => {
    const result = new window.EventTarget();
    result.matches = query.includes('max-width') ? compact : reduced;
    queries.set(query, result);
    return result;
  };
  const intersections = [];
  window.IntersectionObserver = class { constructor(callback) { intersections.push(callback); } observe() {} };
  window.ResizeObserver = class { observe() {} };
  const matches = window.Element.prototype.matches;
  window.Element.prototype.matches = function (selector) { return selector === ':popover-open' ? !!this.__popover : matches.call(this, selector); };
  window.HTMLElement.prototype.showPopover = function () { this.__popover = true; };
  window.HTMLElement.prototype.hidePopover = function () { this.__popover = false; };
  window.HTMLElement.prototype.scrollIntoView = () => {};
  window.eval(controllerBuild.outputFiles[0].text);
  const root = window.document.querySelector('[data-tutorial-demo]');
  const frame = () => root.querySelector('iframe');
  const commands = [];
  const connect = () => {
    const source = frame().contentWindow;
    source.postMessage = data => commands.push(data.command);
    return source;
  };
  const report = (mode, source = frame().contentWindow) => window.dispatchEvent(new window.MessageEvent('message', {
    origin: window.location.origin, source,
    data: { channel: 'ncd-docs-demo:chat', type: 'state', mode, step: 0, title: '第一步', detail: '步骤说明' },
  }));
  t.after(() => window.close());
  return {
    root, window, frame, connect, report, clock, commands,
    click(name) { root.querySelector(`[data-fbd-${name}]`).click(); },
    scroll() { window.dispatchEvent(new window.Event('scroll')); },
    visible(ratio, index = 0) { intersections[index]([{ intersectionRatio: ratio, isIntersecting: ratio > 0 }]); },
    hidden(value) {
      Object.defineProperty(window.document, 'hidden', { configurable: true, value });
      window.document.dispatchEvent(new window.Event('visibilitychange'));
    },
    focused(value) {
      focused = value;
      window.dispatchEvent(new window.Event(value ? 'focus' : 'blur'));
      clock.advance(0);
    },
    motion(value) {
      const query = queries.get('(prefers-reduced-motion: reduce)');
      query.matches = value;
      query.dispatchEvent(new window.Event('change'));
    },
  };
}

test('小屏保持封面，自己操作仍按需加载且不打开全屏', t => {
  const demo = controller(t, { compact: true });
  demo.visible(.8);
  assert.equal(demo.frame().hasAttribute('src'), false);
  demo.click('manual');
  assert.match(demo.frame().src, /lesson=chat/);
  demo.connect();
  demo.report('ready');
  assert.equal(demo.root.dataset.mode, 'manual');
  assert.notEqual(demo.root.dataset.expanded, 'true');
  const manualFrame = demo.frame();
  demo.visible(0);
  demo.clock.advance(60_000);
  assert.equal(demo.frame(), manualFrame);
  assert.equal(demo.root.dataset.mode, 'manual');
});

test('只露出部分窗口时，主动播放不会被自动预览的进入阈值阻止', t => {
  const demo = controller(t, { compact: true });
  demo.visible(.2);
  demo.click('play');
  demo.connect();
  demo.report('ready');
  assert.equal(demo.root.dataset.mode, 'playing');
  demo.visible(.1);
  assert.equal(demo.root.dataset.mode, 'playing');
  demo.visible(0);
  assert.equal(demo.root.dataset.mode, 'paused');
});

test('离屏暂停、重入继续；回收后旧文档消息不能污染新一轮', t => {
  const demo = controller(t);
  demo.visible(.6);
  const firstWindow = demo.connect();
  demo.report('ready');
  assert.equal(demo.root.dataset.mode, 'playing');
  const firstFrame = demo.frame();
  demo.visible(.2);
  assert.equal(demo.root.dataset.mode, 'playing');
  demo.visible(.1);
  assert.equal(demo.root.dataset.mode, 'paused');
  demo.clock.advance(1_000);
  demo.visible(.4);
  assert.equal(demo.frame(), firstFrame);
  assert.equal(demo.root.dataset.mode, 'playing');
  demo.visible(0);
  demo.clock.advance(12_000);
  assert.equal(demo.root.dataset.mode, 'idle');
  assert.equal(demo.frame().hasAttribute('src'), false);
  demo.visible(.5);
  assert.notEqual(demo.frame(), firstFrame);
  demo.report('ready', firstWindow);
  assert.equal(demo.root.dataset.mode, 'loading');
  demo.connect();
  demo.report('ready');
  assert.equal(demo.root.dataset.mode, 'playing');
});

test('主动暂停保留继续点，后台与减少动态效果暂停自动播放', t => {
  const demo = controller(t);
  demo.visible(.8);
  demo.connect();
  demo.report('ready');
  demo.hidden(true);
  assert.equal(demo.root.dataset.mode, 'paused');
  demo.hidden(false);
  assert.equal(demo.root.dataset.mode, 'playing');
  demo.motion(true);
  assert.equal(demo.root.dataset.mode, 'paused');
  demo.click('play');
  assert.equal(demo.root.dataset.mode, 'playing');
  demo.click('play');
  const pausedFrame = demo.frame();
  demo.visible(0);
  demo.clock.advance(60_000);
  demo.visible(.8);
  assert.equal(demo.frame(), pausedFrame);
  assert.equal(demo.root.dataset.mode, 'paused');
  demo.click('play');
  assert.equal(demo.frame(), pausedFrame);
  assert.equal(demo.root.dataset.mode, 'playing');
});

test('展开中的演示不会被回收，收起后恢复正常回收', t => {
  const demo = controller(t);
  demo.visible(.8);
  demo.connect();
  demo.report('ready');
  demo.click('expand');
  const expandedFrame = demo.frame();
  demo.visible(0);
  demo.clock.advance(60_000);
  assert.equal(demo.frame(), expandedFrame);
  assert.equal(demo.root.dataset.expanded, 'true');
  demo.click('expand');
  demo.clock.advance(12_000);
  assert.equal(demo.root.dataset.mode, 'idle');
});

test('展开窗口遮住的自动演示暂停并可回收，收起后才重新加载', t => {
  const demo = controller(t, { count: 2 });
  demo.visible(.8);
  demo.connect();
  demo.report('ready');
  demo.visible(.8, 1);
  const secondRoot = demo.window.document.querySelectorAll('[data-tutorial-demo]')[1];
  secondRoot.querySelector('[data-fbd-play]').click();
  demo.report('ready', secondRoot.querySelector('iframe').contentWindow);
  assert.equal(secondRoot.dataset.mode, 'playing');
  demo.click('expand');
  assert.equal(secondRoot.dataset.mode, 'paused');
  demo.clock.advance(12_000);
  assert.equal(secondRoot.dataset.mode, 'idle');
  assert.equal(demo.root.dataset.mode, 'playing');
  demo.click('expand');
  assert.equal(secondRoot.dataset.mode, 'idle');
  demo.visible(0);
  demo.clock.advance(0);
  assert.equal(secondRoot.dataset.mode, 'loading');
});

test('同时可见的预览只有一个播放，主动播放让位且旧状态不能抢回焦点', t => {
  const demo = controller(t, { count: 2 });
  demo.visible(.8);
  demo.connect();
  demo.report('ready');
  assert.equal(demo.frame().inert, false);
  assert.equal(demo.frame().style.pointerEvents, 'none');
  demo.visible(.8, 1);
  const second = demo.window.document.querySelectorAll('[data-tutorial-demo]')[1];
  assert.equal(second.querySelector('iframe').hasAttribute('src'), false);
  second.querySelector('[data-fbd-play]').click();
  const secondWindow = second.querySelector('iframe').contentWindow;
  demo.report('ready', secondWindow);
  assert.equal(second.dataset.mode, 'playing');
  assert.equal(demo.root.dataset.mode, 'paused');
  assert.equal(demo.frame().inert, true);
  demo.report('playing');
  assert.equal(demo.root.dataset.mode, 'paused');
  second.querySelector('[data-fbd-manual]').click();
  demo.clock.advance(0);
  assert.equal(second.dataset.mode, 'manual');
  assert.equal(second.querySelector('iframe').inert, false);
  assert.equal(second.querySelector('iframe').style.pointerEvents, '');
  assert.equal(demo.root.dataset.mode, 'paused');
});

test('窗口失焦暂停，iframe 获焦不暂停，父页面输入时不抢焦点', t => {
  const demo = controller(t);
  demo.visible(.8);
  demo.connect();
  demo.report('ready');
  demo.focused(false);
  assert.equal(demo.root.dataset.mode, 'paused');
  demo.clock.advance(5_000);
  assert.equal(demo.root.dataset.mode, 'paused');
  demo.focused(true);
  assert.equal(demo.root.dataset.mode, 'playing');
  demo.window.dispatchEvent(new demo.window.Event('blur'));
  demo.clock.advance(0);
  assert.equal(demo.root.dataset.mode, 'playing');
  const input = demo.window.document.createElement('input');
  demo.window.document.body.append(input);
  input.focus();
  assert.equal(demo.root.dataset.mode, 'paused');
  assert.equal(demo.window.document.activeElement, input);
  input.blur();
  demo.clock.advance(0);
  assert.equal(demo.root.dataset.mode, 'playing');
});

test('手动窗口让位后禁用交互，延迟的 manual 回报不能夺回播放权', t => {
  const demo = controller(t, { count: 2 });
  demo.visible(.8);
  demo.click('manual');
  demo.connect();
  demo.report('ready');
  assert.equal(demo.root.dataset.mode, 'manual');
  assert.equal(demo.frame().inert, false);
  demo.visible(.8, 1);
  const second = demo.window.document.querySelectorAll('[data-tutorial-demo]')[1];
  second.querySelector('[data-fbd-play]').click();
  demo.report('ready', second.querySelector('iframe').contentWindow);
  assert.equal(second.dataset.mode, 'playing');
  assert.equal(demo.frame().inert, true);
  assert.equal(demo.frame().style.pointerEvents, 'none');
  demo.report('manual');
  assert.equal(second.dataset.mode, 'playing');
  assert.equal(demo.frame().inert, true);
  demo.click('manual');
  assert.equal(second.dataset.mode, 'paused');
  assert.equal(demo.frame().inert, false);
});

test('滚动时经过演示区不启动真实前端，停稳后才按可见状态加载', t => {
  const demo = controller(t);
  demo.scroll();
  demo.visible(.8);
  assert.equal(demo.frame().hasAttribute('src'), false);
  demo.clock.advance(150);
  demo.scroll();
  demo.clock.advance(159);
  assert.equal(demo.frame().hasAttribute('src'), false);
  demo.clock.advance(1);
  assert.match(demo.frame().src, /lesson=chat/);
  demo.connect();
  demo.report('ready');
  assert.equal(demo.root.dataset.mode, 'playing');
});

test('自动演示滚动时暂停，停稳后在原会话继续；已经离屏则保持暂停', t => {
  const demo = controller(t);
  demo.visible(.8);
  demo.connect();
  demo.report('ready');
  const originalFrame = demo.frame();
  demo.scroll();
  assert.equal(demo.root.dataset.mode, 'paused');
  const pauses = demo.commands.filter(command => command === 'pause').length;
  demo.scroll();
  demo.clock.advance(159);
  assert.equal(demo.root.dataset.mode, 'paused');
  assert.equal(demo.commands.filter(command => command === 'pause').length, pauses);
  demo.clock.advance(1);
  assert.equal(demo.root.dataset.mode, 'playing');
  assert.equal(demo.frame(), originalFrame);
  demo.scroll();
  demo.visible(0);
  demo.clock.advance(160);
  assert.equal(demo.root.dataset.mode, 'paused');
  demo.visible(.8);
  assert.equal(demo.root.dataset.mode, 'playing');
});

test('加载中的自动演示遇到滚动，ready 后等到停稳才开始播放', t => {
  const demo = controller(t);
  demo.visible(.8);
  demo.scroll();
  demo.connect();
  demo.report('ready');
  assert.equal(demo.root.dataset.mode, 'paused');
  assert.equal(demo.commands.includes('play'), false);
  demo.clock.advance(160);
  assert.equal(demo.root.dataset.mode, 'playing');
});

test('显式播放和自己操作绕过滚动调度，继续滚动也不打断', t => {
  const demo = controller(t);
  demo.scroll();
  demo.visible(.8);
  demo.click('play');
  assert.match(demo.frame().src, /lesson=chat/);
  demo.connect();
  demo.report('ready');
  assert.equal(demo.root.dataset.mode, 'playing');
  demo.scroll();
  assert.equal(demo.root.dataset.mode, 'playing');
  demo.click('manual');
  assert.equal(demo.root.dataset.mode, 'manual');
  const manualFrame = demo.frame();
  demo.scroll();
  demo.clock.advance(160);
  assert.equal(demo.root.dataset.mode, 'manual');
  assert.equal(demo.frame(), manualFrame);
});

test('自动演示因滚动暂停后放大，立即继续并不再受滚动调度打断', t => {
  const demo = controller(t);
  demo.visible(.8);
  demo.connect();
  demo.report('ready');
  demo.scroll();
  assert.equal(demo.root.dataset.mode, 'paused');
  demo.click('expand');
  assert.equal(demo.root.dataset.expanded, 'true');
  assert.equal(demo.root.dataset.mode, 'playing');
  demo.scroll();
  assert.equal(demo.root.dataset.mode, 'playing');
});

test('文档中的教程播放不采用首页滚动调度', t => {
  const demo = controller(t, { showcase: false });
  demo.visible(.8);
  demo.click('play');
  demo.connect();
  demo.report('ready');
  assert.equal(demo.root.dataset.mode, 'playing');
  demo.scroll();
  demo.clock.advance(160);
  assert.equal(demo.root.dataset.mode, 'playing');
});
