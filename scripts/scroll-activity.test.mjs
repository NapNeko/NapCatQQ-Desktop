import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const webRoot = fileURLToPath(new URL('../', import.meta.url));
const dependencyRoot = process.env.DESKTOP_DEMO_NODE_MODULES
  ? path.dirname(path.resolve(process.env.DESKTOP_DEMO_NODE_MODULES)) : path.join(webRoot, '.cache/desktop');
const desktopRequire = createRequire(path.join(dependencyRoot, 'package.json'));
const { build } = createRequire(desktopRequire.resolve('vite'))('esbuild');
const compiled = await build({
  entryPoints: [path.join(webRoot, 'src/scripts/scroll-activity.ts')],
  bundle: true, platform: 'browser', format: 'iife', globalName: 'ScrollActivityTest',
  write: false, logLevel: 'silent', target: 'es2022',
});

function activity() {
  let now = 0;
  let nextTimer = 0;
  const timers = new Map();
  const handlers = new Set();
  const registrations = [];
  let removals = 0;
  const window = {
    setTimeout(callback, delay) { const id = ++nextTimer; timers.set(id, { at: now + delay, callback }); return id; },
    clearTimeout(id) { timers.delete(id); },
    addEventListener(type, listener, options) {
      registrations.push({ type, options });
      handlers.add(listener);
    },
    removeEventListener(type, listener) {
      assert.equal(type, 'scroll');
      handlers.delete(listener);
      removals += 1;
    },
  };
  const api = Function('window', `${compiled.outputFiles[0].text}; return ScrollActivityTest;`)(window);
  return {
    ...api, registrations,
    get removals() { return removals; },
    get pendingTimers() { return timers.size; },
    scroll() { for (const handler of [...handlers]) handler(); },
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

test('共享一个 passive 监听，连续滚动只通知一次忙碌并在最后一次滚动 160ms 后恢复', () => {
  const scroll = activity();
  const first = [], second = [];
  assert.equal(scroll.registrations.length, 0);
  scroll.subscribeScrollActivity(value => first.push(value));
  scroll.subscribeScrollActivity(value => second.push(value));
  assert.deepEqual(scroll.registrations, [{ type: 'scroll', options: { passive: true } }]);
  scroll.scroll();
  scroll.advance(100);
  scroll.scroll();
  scroll.advance(159);
  assert.equal(scroll.isScrollActive(), true);
  assert.deepEqual(first, [false, true]);
  assert.deepEqual(second, first);
  assert.equal(scroll.pendingTimers, 1);
  scroll.advance(1);
  assert.equal(scroll.isScrollActive(), false);
  assert.deepEqual(first, [false, true, false]);
  assert.deepEqual(second, first);
});

test('最后一个订阅释放监听和 idle 任务，重新订阅从空闲状态开始', () => {
  const scroll = activity();
  const first = [], second = [];
  const unsubscribeFirst = scroll.subscribeScrollActivity(value => first.push(value));
  const unsubscribeSecond = scroll.subscribeScrollActivity(value => second.push(value));
  scroll.scroll();
  unsubscribeFirst();
  assert.equal(scroll.removals, 0);
  scroll.advance(160);
  assert.deepEqual(first, [false, true]);
  assert.deepEqual(second, [false, true, false]);
  scroll.scroll();
  unsubscribeSecond();
  unsubscribeSecond();
  assert.equal(scroll.removals, 1);
  assert.equal(scroll.pendingTimers, 0);
  assert.equal(scroll.isScrollActive(), false);
  scroll.advance(1000);
  assert.deepEqual(second, [false, true, false, true]);
  const restored = [];
  scroll.subscribeScrollActivity(value => restored.push(value));
  assert.deepEqual(restored, [false]);
  scroll.scroll();
  scroll.advance(160);
  assert.deepEqual(restored, [false, true, false]);
});
