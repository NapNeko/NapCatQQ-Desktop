import assert from 'node:assert/strict';
import { readdir, readFile } from 'node:fs/promises';
import path from 'node:path';
import test from 'node:test';
import config from '../astro.config.mjs';

const documents = path.resolve(import.meta.dirname, '../src/content/docs');
const renderer = await config.markdown.processor.createRenderer({ syntaxHighlight: false });
const expectedLabels = {
  'remote-login.md': ['Windows 远端：', 'Linux：', 'QQ 窗口：', '刚开启功能：'],
  'remote-linux.md': ['添加主机时一起配置：', '完成后核对三件事：'],
  'remote-windows.md': ['SSH 能连，代理安装失败：', '服务在运行，QQ 一直等会话：', '重连后状态看起来不对：'],
  'troubleshooting.md': ['连接方向正确吗？', '应用端开着吗？', '地址属于哪台机器？', '端口可到达吗？', 'token 和路径一致吗？', '双方使用相同的 OneBot 版本和连接角色吗？', '配置保存并加载了吗？'],
};

test('document emphasis renders through the configured Markdown processor', async () => {
  for (const file of (await readdir(documents)).filter(file => file.endsWith('.md'))) {
    const source = await readFile(path.join(documents, file), 'utf8');
    const { code } = await renderer.render(source.replace(/^---\r?\n[\s\S]*?\r?\n---\r?\n/, ''));
    const prose = code.replace(/<pre\b[^>]*>[\s\S]*?<\/pre>/g, '').replace(/<code\b[^>]*>[\s\S]*?<\/code>/g, '');
    assert.ok(!prose.includes('**'), `${file}: unrendered emphasis remains in document prose`);
    for (const label of expectedLabels[file] || []) {
      assert.ok(code.includes(`<strong>${label}</strong>`), `${file}: missing bold label ${label}`);
    }
  }
});

test('screenshots expose one theme-controlled image with a no-script fallback', async () => {
  const { code } = await renderer.render('![组件页](/docs/screenshots/components-light.png)');
  assert.match(code, /data-theme-image=/);
  assert.match(code, /components-light/);
  assert.match(code, /components-dark/);
  assert.match(code, /<noscript><img\b/);
  const live = code.replace(/<noscript>[\s\S]*?<\/noscript>/g, '');
  assert.equal((live.match(/<img\b/g) || []).length, 1);
  assert.match(live, /src="data:image\/svg\+xml/);
  assert.doesNotMatch(live, /\ssrcset=/);
  assert.doesNotMatch(code, /data-shot-theme|data-mode|shot-switch/);
});

test('tables keep their native layout inside a keyboard-accessible scroll region', async () => {
  const { code } = await renderer.render('| 项目 | 这次填写什么 | 为什么 |\n| --- | --- | --- |\n| 实例名称 | 例如 `我的第一个 Bot` | 便于辨认 |');
  assert.match(code, /<div\b[^>]*class="doc-table"[^>]*tabindex="0"[^>]*role="region"[^>]*aria-label="表格，可左右滚动"[^>]*>\s*<table\b/);
  assert.equal((code.match(/<table\b/g) || []).length, 1);
  assert.match(code, /<thead>[\s\S]*<th>项目<\/th>/);
  assert.match(code, /<td>例如 <code>我的第一个 Bot<\/code><\/td>/);
  assert.match(code, /<\/table>\s*<\/div>/);
});
