import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const demoRoot = fileURLToPath(new URL('./', import.meta.url));
const desktopRoot = path.resolve(demoRoot, '../.cache/desktop');
const dependencyRoot = process.env.DESKTOP_DEMO_NODE_MODULES
  ? path.dirname(path.resolve(process.env.DESKTOP_DEMO_NODE_MODULES)) : desktopRoot;
const desktopRequire = createRequire(path.join(dependencyRoot, 'package.json'));
const viteRequire = createRequire(desktopRequire.resolve('vite'));
const { build } = viteRequire('esbuild');
const { JSDOM } = await import(pathToFileURL(desktopRequire.resolve('jsdom')).href);
const compiled = await build({
  stdin: {
    contents: `
      export { installPlayer } from './player.ts';
      import React from 'react';
      import { createRoot } from 'react-dom/client';
      import * as Select from '@radix-ui/react-select';
      export function mountSelectFixture(container, onValueChange) {
        function Fixture() {
          const [value, setValue] = React.useState('system');
          return React.createElement(Select.Root, {
            value,
            onValueChange(next) { setValue(next); onValueChange(next); },
          },
            React.createElement(Select.Trigger, { id: 'proxy-mode' }, React.createElement(Select.Value)),
            React.createElement(Select.Portal, null,
              React.createElement(Select.Content, { position: 'popper' },
                React.createElement(Select.Viewport, null,
                  ['system', 'manual'].map(option => React.createElement(Select.Item, { key: option, value: option },
                    React.createElement(Select.ItemText, null, option)))))));
        }
        const root = createRoot(container);
        root.render(React.createElement(Fixture));
        return () => root.unmount();
      }
    `,
    resolveDir: demoRoot,
    loader: 'ts',
  },
  // Pin peers as well as the fixture imports; pnpm may otherwise resolve a second React via the parent repository.
  alias: {
    '@desktop': path.join(desktopRoot, 'src-ui'),
    react: path.join(dependencyRoot, 'node_modules/react'),
    'react-dom': path.join(dependencyRoot, 'node_modules/react-dom'),
    '@radix-ui/react-select': path.join(dependencyRoot, 'node_modules/@radix-ui/react-select'),
  },
  nodePaths: [path.join(dependencyRoot, 'node_modules')],
  bundle: true,
  platform: 'browser',
  format: 'iife',
  globalName: 'DemoPlayerTest',
  target: 'es2022',
  write: false,
  logLevel: 'silent',
});
const source = compiled.outputFiles[0].text;
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));

async function until(check, message, timeout = 2500) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if (check()) return;
    await delay(10);
  }
  assert.fail(message);
}

function fixture(t, actions, { observe, onReset = () => {}, onTheme = () => {} } = {}) {
  const dom = new JSDOM('<!doctype html><div id="root"><div class="demo-cursor" hidden></div></div>', {
    url: 'http://127.0.0.1:4321/demo/index.html',
    runScripts: 'outside-only',
    pretendToBeVisual: true,
  });
  const { window } = dom;
  const cleanups = [];
  // JSDOM has no layout or PointerEvent; supply browser geometry without replacing component behavior.
  window.HTMLElement.prototype.getClientRects = function () { return this.isConnected ? [this.getBoundingClientRect()] : []; };
  window.HTMLElement.prototype.getBoundingClientRect = function () {
    const top = this.getAttribute('role') === 'option' ? 160 : 100;
    return { x: 100, y: top, top, left: 100, right: 300, bottom: top + 30, width: 200, height: 30, toJSON() {} };
  };
  window.HTMLElement.prototype.scrollIntoView = function () {};
  window.HTMLElement.prototype.hasPointerCapture = () => false;
  window.HTMLElement.prototype.releasePointerCapture = function () {};
  window.PointerEvent = class extends window.MouseEvent {
    constructor(type, options = {}) {
      super(type, options);
      Object.defineProperties(this, {
        pointerId: { value: options.pointerId ?? 1 },
        pointerType: { value: options.pointerType ?? 'mouse' },
      });
    }
  };
  window.ResizeObserver = class { observe() {} unobserve() {} disconnect() {} };
  const reports = [];
  const channel = 'ncd-docs-demo:state-test';
  window.postMessage = (data, targetOrigin) => {
    assert.equal(targetOrigin, window.location.origin);
    reports.push({ ...data });
  };
  const playerApi = window.eval(`${source}\nDemoPlayerTest;`);
  const dispose = playerApi.installPlayer({
    id: 'state-test', route: 'bots',
    steps: actions.map((_, index) => [`步骤 ${index + 1}`, `测试操作 ${index + 1}`]),
    reset() {}, actions, observe,
  }, { reset: () => onReset(window), theme: onTheme });
  t.after(() => { dispose(); cleanups.reverse().forEach(cleanup => cleanup()); window.close(); });
  return {
    window,
    api: playerApi,
    cleanup: callback => cleanups.push(callback),
    reports,
    state: () => reports.filter(report => report.type === 'state').at(-1),
    command(command, overrides = {}) {
      const { origin = window.location.origin, source = window.parent, ...data } = overrides;
      window.dispatchEvent(new window.MessageEvent('message', {
        origin, source,
        data: { channel, type: 'command', command, ...data },
      }));
    },
  };
}

test('异步操作暂停后不继续写入，恢复播放不重复执行已进入的动作', async t => {
  const calls = [];
  const player = fixture(t, [
    async driver => {
      calls.push('first:start');
      await driver.wait(350);
      calls.push('first:end');
    },
    async driver => {
      calls.push('second:start');
      await driver.wait(80);
      calls.push('second:end');
    },
  ]);
  player.command('play');
  await until(() => calls.length === 1, '首个动作没有开始');
  player.command('pause');
  await delay(500);
  assert.equal(player.state().mode, 'paused');
  assert.deepEqual(calls, ['first:start']);

  player.command('play');
  await until(() => player.state().mode === 'complete', '恢复后没有完成');
  assert.deepEqual(calls, ['first:start', 'first:end', 'second:start', 'second:end']);
  assert.equal(player.state().step, 1);
});

test('手动接管取消未完成动作，再次演示从新场景开始且旧动作不会补写', async t => {
  const calls = [];
  let resets = 0;
  const player = fixture(t, [
    async driver => {
      calls.push('first:start');
      await driver.wait(350);
      calls.push('first:end');
    },
    async driver => { await driver.wait(40); calls.push('second'); },
  ], { observe: () => 1, onReset: () => { resets += 1; } });
  player.command('play');
  await until(() => calls.length === 1, '首个动作没有开始');
  player.command('manual');
  await delay(450);
  assert.equal(player.state().mode, 'manual');
  assert.equal(player.state().step, 1);
  assert.deepEqual(calls, ['first:start']);

  player.command('play');
  await until(() => player.state().mode === 'complete', '重新演示没有完成');
  assert.equal(resets, 1);
  assert.deepEqual(calls, ['first:start', 'first:start', 'first:end', 'second']);
});

test('重置取消旧 generation，立即重播不会被旧等待恢复后的写入覆盖', async t => {
  const writes = [];
  let runs = 0;
  let resets = 0;
  const player = fixture(t, [async driver => {
    const run = ++runs;
    await driver.wait(run === 1 ? 480 : 90);
    writes.push(run);
  }], { onReset: () => { resets += 1; writes.splice(0); } });
  player.command('play');
  await until(() => runs === 1, '旧动作没有开始');
  player.command('reset');
  assert.equal(player.state().mode, 'ready');
  assert.equal(player.state().step, 0);
  assert.deepEqual(writes, []);

  player.command('play');
  await until(() => player.state().mode === 'complete', '新一轮没有完成');
  await delay(500);
  assert.equal(resets, 1);
  assert.equal(runs, 2);
  assert.deepEqual(writes, [2]);
  assert.equal(player.state().mode, 'complete');
  assert.equal(player.reports.some(report => report.mode === 'error'), false);
});

test('只接受正确父窗口、origin、channel 和命令类型的消息', async t => {
  const calls = [];
  const themes = [];
  let resets = 0;
  const player = fixture(t, [async driver => { await driver.wait(40); calls.push('ran'); }], {
    onReset: () => { resets += 1; },
    onTheme: theme => themes.push(theme),
  });
  const initialReports = player.reports.length;
  player.command('play', { source: null });
  player.command('manual', { origin: 'https://unrelated.example' });
  player.command('play', { channel: 'ncd-docs-demo:another-lesson' });
  player.command('reset', { channel: 'ncd-docs-demo:another-lesson' });
  player.command('theme', { type: 'state', theme: 'dark' });
  await delay(300);
  assert.equal(player.reports.length, initialReports);
  assert.equal(player.state().mode, 'ready');
  assert.deepEqual(calls, []);
  assert.deepEqual(themes, []);
  assert.equal(resets, 0);

  player.command('theme', { theme: 'dark' });
  assert.deepEqual(themes, ['dark']);
  player.command('play');
  await until(() => player.state().mode === 'complete', '正确消息没有触发演示');
  assert.deepEqual(calls, ['ran']);
});

test('手动模式重置时，同步 input 通知不能按旧页面把进度改回末步', async t => {
  let resets = 0;
  const player = fixture(t, [async () => {}, async () => {}], {
    observe: () => 1,
    onReset: window => {
      resets += 1;
      window.document.dispatchEvent(new window.Event('input', { bubbles: true }));
    },
  });
  player.command('manual');
  assert.equal(player.state().mode, 'manual');
  assert.equal(player.state().step, 1);

  player.command('reset');
  assert.equal(resets, 1);
  assert.equal(player.state().mode, 'ready');
  assert.equal(player.state().step, 0);
  await delay(30);
  assert.equal(player.state().mode, 'ready');
  assert.equal(player.state().step, 0);
});

test('暂停期间替换控件后，点击和填写只作用于重新取得的节点', async t => {
  const calls = [];
  const values = [];
  const player = fixture(t, [
    async driver => { await driver.click(driver.by('#replace-button')); },
    async driver => { await driver.type(driver.by('#replace-input'), '7890'); },
  ]);
  const { document } = player.window;
  const button = document.createElement('button');
  button.id = 'replace-button';
  button.addEventListener('click', () => calls.push('detached'));
  const input = document.createElement('input');
  input.id = 'replace-input';
  input.addEventListener('input', () => values.push('detached'));
  document.getElementById('root').append(button, input);

  player.command('play');
  await until(() => button.hasAttribute('data-demo-target'), '按钮未进入点击前的指向阶段');
  player.command('pause');
  const replacementButton = document.createElement('button');
  replacementButton.id = button.id;
  replacementButton.addEventListener('click', () => calls.push('current'));
  button.replaceWith(replacementButton);
  await delay(120);
  assert.deepEqual(calls, []);
  player.command('play');
  await until(() => input.hasAttribute('data-demo-target'), '文本控件未进入填写前的指向阶段');
  assert.deepEqual(calls, ['current']);

  player.command('pause');
  const replacementInput = document.createElement('textarea');
  replacementInput.id = input.id;
  replacementInput.addEventListener('input', () => values.push(replacementInput.value));
  input.replaceWith(replacementInput);
  await delay(120);
  assert.deepEqual(values, []);
  player.command('play');
  await until(() => player.state().mode === 'complete', '替换控件后演示未完成', 4000);
  assert.deepEqual(calls, ['current']);
  assert.equal(values.includes('detached'), false);
  assert.equal(values.at(-1), '7890');
  assert.equal(replacementInput.value, '7890');
  assert.equal(input.value, '');
});

test('真实 Radix 菜单在暂停时因 blur 关闭，继续后重新打开并只提交一次选择', async t => {
  const changes = [];
  const player = fixture(t, [async driver => {
    await driver.select(driver.by('#proxy-mode'), driver.text('manual', '[role="option"]'));
  }]);
  const { window } = player;
  const container = window.document.createElement('div');
  window.document.getElementById('root').append(container);
  player.cleanup(player.api.mountSelectFixture(container, value => changes.push(value)));
  await until(() => window.document.getElementById('proxy-mode'), '真实 Select 没有挂载');
  player.command('play');
  await until(() => window.document.querySelector('[role="option"][data-demo-target]'), '未进入菜单选项的指向阶段', 4000);
  player.command('pause');
  window.dispatchEvent(new window.Event('blur'));
  await until(() => window.document.getElementById('proxy-mode').getAttribute('aria-expanded') === 'false', '真实 Radix 未在 blur 时关闭');
  assert.deepEqual(changes, []);
  await delay(120);
  player.command('play');
  await until(() => player.state().mode === 'complete', '继续后未恢复下拉选择', 5000);
  assert.deepEqual(changes, ['manual']);
  assert.equal(window.document.getElementById('proxy-mode').textContent, 'manual');
  assert.equal(player.reports.some(report => report.mode === 'error'), false);
});
