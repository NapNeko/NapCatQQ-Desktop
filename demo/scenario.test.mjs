import assert from 'node:assert/strict';
import { afterEach, test } from 'node:test';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const demoRoot = fileURLToPath(new URL('./', import.meta.url));
const desktopRoot = path.resolve(demoRoot, '../.cache/desktop');
const dependencyRoot = process.env.DESKTOP_DEMO_NODE_MODULES
  ? path.dirname(path.resolve(process.env.DESKTOP_DEMO_NODE_MODULES)) : desktopRoot;
const desktopRequire = createRequire(path.join(dependencyRoot, 'package.json'));
const viteRequire = createRequire(desktopRequire.resolve('vite'));
const { build } = viteRequire('esbuild');
const compiled = await build({
  stdin: {
    contents: 'export * from "./bot.mock.ts"; export { subscribeMockEvents } from "./events.mock.ts";',
    resolveDir: demoRoot,
    loader: 'ts',
  },
  alias: { '@desktop': path.join(desktopRoot, 'src-ui') },
  bundle: true,
  platform: 'node',
  format: 'esm',
  target: 'node22',
  write: false,
  logLevel: 'silent',
});
const scenario = await import(`data:text/javascript;base64,${Buffer.from(compiled.outputFiles[0].contents).toString('base64')}`);
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));

function config(id = '10003') {
  const value = scenario.buildMockBotConfig(id);
  value.bot.QQID = Number(id);
  value.bot.name = '我的第一个 Bot';
  return value;
}

afterEach(() => { scenario.resetScenario(); });

test('保存完整配置，并隔离调用方与读取方的后续修改', async () => {
  const original = config();
  original.bot.backend_type = 'snowluma';
  original.connect.httpServers.push({
    enable: true, name: '教程 HTTP', messagePostFormat: 'array', token: 'example-only',
    debug: false, host: '127.0.0.1', port: 3000, enableCors: false,
    enableWebsocket: false, path: '/',
  });
  const expected = structuredClone(original);
  const snapshot = await scenario.mockUpsertBotConfig(original);
  original.bot.name = '调用后修改';
  original.connect.httpServers[0].port = 9999;
  snapshot.state = 'running';
  assert.deepEqual(scenario.demoReadConfig('10003'), expected);
  assert.equal(scenario.mockSnapshots[0].state, 'stopped');
  assert.equal(scenario.demoFlavors()['10003'], 'snowluma');

  const returned = scenario.demoReadConfig('10003');
  returned.bot.name = '读取后修改';
  returned.connect.httpServers.splice(0);
  assert.deepEqual(scenario.demoReadConfig('10003'), expected);
  expected.bot.name = '第二次保存';
  await scenario.mockUpsertBotConfig(expected);
  assert.equal(scenario.mockSnapshots.length, 1);
  assert.equal(scenario.demoReadConfig('10003').bot.name, '第二次保存');
});

test('新账号启动仅报告进程运行，日志归属该账号且不报告 QQ 在线', async () => {
  const events = [];
  const unsubscribe = scenario.subscribeMockEvents(event => events.push(event));
  try {
    await scenario.mockUpsertBotConfig(config('20868'));
    const starting = await scenario.mockStartBot('20868');
    assert.equal(starting.state, 'starting');
    await delay(2250);
    assert.equal(scenario.mockSnapshots[0].state, 'running');
    assert.equal(scenario.mockBots[0].state, 'running');
    assert.ok(events.some(event => event.kind === 'bot_state_changed' && event.snapshot.state === 'running'));
    assert.equal(events.some(event => event.kind === 'napcat_login_online' || event.kind === 'snowluma_login_state_changed'), false);
    const logEvents = events.filter(event => event.kind === 'bot_log_appended');
    assert.ok(logEvents.length >= 5);
    assert.ok(logEvents.every(event => event.bot_id === '20868'));
    const log = scenario.demoTailLog('20868');
    assert.match(log.lines.at(-1), /等待 QQ 登录/);
    assert.deepEqual(log.lines, logEvents.map(event => event.line));
    assert.deepEqual(scenario.demoTailLog('10001'), { lines: [], total_lines: 0 });
  } finally { unsubscribe(); }
});

test('停止、删除和重置会取消旧启动任务，同一账号重建后不会被旧任务改写', async () => {
  const events = [];
  const unsubscribe = scenario.subscribeMockEvents(event => events.push(event));
  const becameRunning = id => events.some(event => event.kind === 'bot_state_changed'
    && event.snapshot.bot_id === id && event.snapshot.state === 'running');
  try {
    await scenario.mockUpsertBotConfig(config('31001'));
    await scenario.mockStartBot('31001');
    await scenario.mockStopBot('31001');
    await delay(2250);
    assert.equal(scenario.mockSnapshots.find(bot => bot.bot_id === '31001').state, 'stopped');
    assert.equal(becameRunning('31001'), false);
    assert.equal(scenario.demoTailLog('31001').lines.some(line => line.includes('协议端进程已启动')), false);

    await scenario.mockUpsertBotConfig(config('31002'));
    await scenario.mockStartBot('31002');
    await scenario.mockDeleteBotConfig('31002');
    assert.equal(scenario.demoReadConfig('31002'), null);
    assert.deepEqual(scenario.demoTailLog('31002'), { lines: [], total_lines: 0 });
    await scenario.mockUpsertBotConfig(config('31002'));
    await delay(2250);
    assert.equal(scenario.mockSnapshots.find(bot => bot.bot_id === '31002').state, 'stopped');
    assert.equal(becameRunning('31002'), false);

    await scenario.mockUpsertBotConfig(config('31003'));
    await scenario.mockStartBot('31003');
    scenario.resetScenario();
    assert.deepEqual(scenario.mockSnapshots, []);
    assert.deepEqual(scenario.mockBots, []);
    assert.equal(scenario.demoReadConfig('31003'), null);
    await scenario.mockUpsertBotConfig(config('31003'));
    await delay(2250);
    assert.equal(scenario.mockSnapshots[0].state, 'stopped');
    assert.equal(becameRunning('31003'), false);
    assert.equal(scenario.demoTailLog('31003').lines.length, 1);
  } finally { unsubscribe(); }
});

test('重置后的两轮创建和启动生成相同日志，返回的日志数组不能改写场景', async () => {
  const runs = [];
  for (let round = 0; round < 2; round += 1) {
    scenario.resetScenario();
    await scenario.mockUpsertBotConfig(config());
    await scenario.mockStartBot('10003');
    await delay(2250);
    runs.push(scenario.demoTailLog('10003'));
    assert.equal(scenario.mockSnapshots.length, 1);
  }
  assert.deepEqual(runs[0], runs[1]);
  assert.equal(runs[0].total_lines, 5);
  runs[1].lines.splice(0);
  assert.equal(scenario.demoTailLog('10003').lines.length, 5);
});

test('预置已有 Bot 后可以编辑配置，保留进程状态与原日志且不共享种子数组', async () => {
  const lines = ['[INFO] [演示] 已有 Bot 的日志'];
  scenario.seedDemoBot('10003', '预置实例', 'running', lines);
  lines[0] = '外部修改';
  assert.equal(scenario.mockSnapshots[0].state, 'running');
  assert.equal(scenario.mockBots[0].state, 'running');
  assert.equal(scenario.demoReadConfig('10003').bot.name, '预置实例');
  assert.deepEqual(scenario.demoTailLog('10003').lines, ['[INFO] [演示] 已有 Bot 的日志']);

  const updated = scenario.demoReadConfig('10003');
  updated.bot.name = '日常值守 Bot';
  updated.bot.offlineAutoRestart = true;
  await scenario.mockUpsertBotConfig(updated);
  assert.equal(scenario.mockSnapshots.length, 1);
  assert.equal(scenario.mockSnapshots[0].state, 'running');
  assert.equal(scenario.demoReadConfig('10003').bot.name, '日常值守 Bot');
  assert.equal(scenario.demoReadConfig('10003').bot.offlineAutoRestart, true);
  assert.deepEqual(scenario.demoTailLog('10003').lines, ['[INFO] [演示] 已有 Bot 的日志']);
});

test('连接、日志和维护场景重置后，同一账号不会残留前一场景的配置、日志或其他账号', async () => {
  scenario.seedDemoBot('10003', 'HTTP 连接示例');
  const connectionConfig = scenario.demoReadConfig('10003');
  connectionConfig.connect.httpServers.push({
    enable: true, name: 'local-http', messagePostFormat: 'array', token: 'demo-token-10003',
    debug: false, host: '127.0.0.1', port: 3000, enableCors: false,
    enableWebsocket: false, path: '/',
  });
  await scenario.mockUpsertBotConfig(connectionConfig);
  scenario.seedDemoBot('10004', '另一份临时实例');
  assert.equal(scenario.demoReadConfig('10003').connect.httpServers.length, 1);

  scenario.resetScenario();
  scenario.seedDemoBot('10003', '日志排查示例', 'running', ['[ERROR] [演示] OneBot 鉴权失败']);
  assert.equal(scenario.mockSnapshots.length, 1);
  assert.equal(scenario.demoReadConfig('10004'), null);
  assert.deepEqual(scenario.demoTailLog('10004'), { lines: [], total_lines: 0 });
  assert.deepEqual(scenario.demoReadConfig('10003').connect.httpServers, []);
  assert.deepEqual(scenario.demoTailLog('10003').lines, ['[ERROR] [演示] OneBot 鉴权失败']);

  scenario.resetScenario();
  scenario.seedDemoBot('10003', '维护示例', 'stopped');
  assert.equal(scenario.demoReadConfig('10003').bot.offlineAutoRestart, false);
  assert.equal(scenario.mockSnapshots[0].state, 'stopped');
  assert.deepEqual(scenario.demoReadConfig('10003').connect.httpServers, []);
  assert.equal(scenario.demoTailLog('10003').lines.some(line => line.includes('鉴权失败')), false);
});
