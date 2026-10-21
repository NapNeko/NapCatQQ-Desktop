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
const canonical = value => path.resolve(value).replaceAll('\\', '/').toLowerCase().replace(/\.ts$/, '');
const replacements = new Map(['bot', 'events'].map(name => [
  canonical(path.join(desktopRoot, `src-ui/core/ipc/mock/${name}.mock`)),
  path.join(demoRoot, `${name}.mock.ts`),
]));
const compiled = await build({
  stdin: {
    contents: `
      export * from './workspace.mock.ts';
      export { instanceApi } from '@desktop/core/ipc/mock/app-framework/instance-api';
      export { linkApi } from '@desktop/core/ipc/mock/app-framework/link-api';
      export { instances } from '@desktop/core/ipc/mock/app-framework/state';
      export { peekKarinHttpAuthKey } from '@desktop/core/ipc/mock/app-config.mock';
      export { chatDesktopService } from '@desktop/core/services/chat-desktop.service';
    `,
    resolveDir: demoRoot,
    loader: 'ts',
  },
  alias: { '@desktop': path.join(desktopRoot, 'src-ui') },
  nodePaths: [path.join(dependencyRoot, 'node_modules')],
  plugins: [{
    name: 'shared-demo-mocks',
    setup(builder) {
      builder.onResolve({ filter: /(?:bot|events)\.mock(?:\.ts)?$/ }, args => {
        if (!args.path.startsWith('.')) return;
        const replacement = replacements.get(canonical(path.resolve(args.resolveDir, args.path)));
        return replacement ? { path: replacement } : undefined;
      });
    },
  }],
  define: { 'import.meta.env': '{"DEV":false,"PROD":true,"MODE":"test"}' },
  bundle: true,
  platform: 'browser',
  format: 'iife',
  globalName: 'DemoWorkspaceTest',
  target: 'es2022',
  write: false,
  logLevel: 'silent',
});
const source = compiled.outputFiles[0].text;

function fixture(t) {
  const dom = new JSDOM('<!doctype html><div id="root"></div>', {
    url: 'http://127.0.0.1:4321/demo/index.html',
    runScripts: 'outside-only',
    pretendToBeVisual: true,
  });
  const { window } = dom;
  window.structuredClone = structuredClone;
  window.matchMedia = () => ({ matches: false, addEventListener() {}, removeEventListener() {} });
  const api = window.eval(`${source}\nDemoWorkspaceTest;`);
  t.after(() => window.close());
  api.resetWorkspaceDemo('apps');
  return api;
}

test('应用配置和原始文件保存后重置，回到种子配置并清除对接 Token', async t => {
  const api = fixture(t);
  const oldId = api.workspaceAppId;
  const initial = await api.instanceApi.readConfig(oldId);
  const originalText = await api.instanceApi.readConfigText(oldId, 'env');
  const changed = structuredClone(initial.config);
  changed.data.env.http_port += 1;
  changed.data.env.http_auth_key = 'example-only-edited-key';
  changed.data.config.master.push('23333');
  await api.instanceApi.writeConfig(oldId, changed, initial.revision);
  assert.equal(api.peekKarinHttpAuthKey(oldId), 'example-only-edited-key');
  await api.linkApi.applyLink(oldId, '10003');
  const linked = await api.instanceApi.readConfig(oldId);
  assert.notEqual(linked.config.data.env.ws_server_auth_key, initial.config.data.env.ws_server_auth_key);
  const env = await api.instanceApi.readConfigText(oldId, 'env');
  await api.instanceApi.writeConfigText(oldId, 'env', '# previous run only\n', env.revision);

  api.resetWorkspaceDemo('apps');
  assert.equal(api.workspaceAppId, oldId);
  assert.equal(api.instances.length, 1);
  assert.equal(api.peekKarinHttpAuthKey(oldId), initial.config.data.env.http_auth_key);
  const reset = await api.instanceApi.readConfig(api.workspaceAppId);
  assert.deepEqual(reset, initial);
  assert.deepEqual(await api.instanceApi.readConfigText(api.workspaceAppId, 'env'), originalText);
  assert.equal(api.instances[0].link, undefined);
  assert.equal(api.instances[0].port, initial.config.data.env.http_port);
});

test('聊天适配延迟加载时使用最新一次重置的账号', async t => {
  const api = fixture(t);
  api.resetWorkspaceDemo('chat');
  api.resetWorkspaceDemo('chat');
  await Promise.all([api.installWorkspaceDemo(), api.installWorkspaceDemo()]);
  const view = await api.chatDesktopService.loadView();
  assert.equal(view.revision, 3);
  assert.equal(view.selectedBot, 'docs-chat-3');
});

test('聊天重复打开保留操作，重置后拒绝旧视图的异步保存', async t => {
  const api = fixture(t);
  api.resetWorkspaceDemo('chat');
  await api.installWorkspaceDemo();
  const original = await api.chatDesktopService.loadView();
  await api.chatDesktopService.saveView({ ...original, selectedBot: 'manual-selection' });
  await api.installWorkspaceDemo();
  assert.equal((await api.chatDesktopService.loadView()).selectedBot, 'manual-selection');

  api.resetWorkspaceDemo('chat');
  await api.chatDesktopService.saveView(original);
  const reset = await api.chatDesktopService.loadView();
  assert.equal(reset.revision, original.revision + 1);
  assert.equal(reset.selectedBot, `docs-chat-${reset.revision}`);
});
