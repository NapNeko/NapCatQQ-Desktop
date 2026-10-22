import assert from 'node:assert/strict';
import test from 'node:test';
import { normalizeReleases } from './sync-releases.mjs';
import { changelogSyncedAt, getChangelog, releaseSummary, renderReleaseMarkdown, splitReleaseBody } from '../src/lib/changelog.ts';

const published = (tag, date = '2026-10-18T09:11:55Z', extra = {}) => ({
  tag_name: tag, name: tag, published_at: date, body: '**原文摘要**\n\n### 修复\n\n- 修复内容',
  html_url: `https://github.com/NapNeko/NapCatQQ-Desktop/releases/tag/${tag}`,
  draft: false, prerelease: false, assets: [], ...extra,
});

test('release snapshots exclude drafts and unrelated tags while preserving published bodies and previews', () => {
  const input = [published('v3.5.2'), published('watch-v0.3.0'), published('v3.6.0-rc.1', '2026-10-19T00:00:00Z', { prerelease: true }), published('v3.5.2'), published('v3.7.0', undefined, { draft: true }), published('nightly')];
  const result = normalizeReleases(input);
  assert.deepEqual(result.map(release => release.tag), ['v3.6.0-rc.1', 'watch-v0.3.0', 'v3.5.2']);
  assert.equal(result[0].prerelease, true);
  assert.equal(result[0].body, input[2].body);
  assert.throws(() => normalizeReleases([published('v3.5.2', 'invalid')]), /Invalid publish date/);
});

test('only generated release sections move to the appendix and the original first paragraph supplies the summary', () => {
  const source = '**本版摘要**。\n\n<!-- ncd-release-notes: download -->\n\n### 下载\n\n安装说明。\n\n<!-- ncd-release-notes: /download -->\n\n### 修复\n\n- 保留原文。\n\n<!-- ncd-release-notes: footer -->\n\n### 全部提交\n\n- 原始提交。';
  const parts = splitReleaseBody(source);
  assert.equal(parts.body, '**本版摘要**。\n\n\n\n### 修复\n\n- 保留原文。');
  assert.ok(parts.appendix.includes('安装说明。'));
  assert.ok(parts.appendix.includes('原始提交。'));
  assert.equal(releaseSummary(parts.body, 'fallback'), '本版摘要。');
  assert.equal(releaseSummary('### 更新\n\n- 条目', 'v3.5.2'), 'v3.5.2');
});

test('release Markdown remains inert and resolves relative source links with per-version heading IDs', async () => {
  const source = `### 修复\n\n**正常粗体** [说明](docs/user/README.md) [本节](#修复) [坏链接](javascript:alert%281%29)\n\n<img src="x" onerror="alert(1)"><script>alert(1)</script><iframe src="https://example.com"></iframe>\n\n\`<script>\`\n\n| 项目 | 内容 |\n| --- | --- |\n| 配置 | 保留表格 |`;
  const html = await renderReleaseMarkdown(source, 'v3.5.2');
  assert.match(html, /<strong>正常粗体<\/strong>/);
  assert.match(html, /id="v3\.5\.2-修复"/);
  assert.match(html, /href="#v3\.5\.2-修复"/);
  assert.match(html, /href="https:\/\/github\.com\/NapNeko\/NapCatQQ-Desktop\/blob\/v3\.5\.2\/docs\/user\/README\.md"/);
  assert.match(html, /<table>/);
  assert.match(html, /&lt;script&gt;/);
  assert.doesNotMatch(html, /<script|<iframe|onerror=|javascript:/i);
});

test('the saved public snapshot builds both product streams and chooses separate stable downloads', async () => {
  assert.ok(!Number.isNaN(Date.parse(changelogSyncedAt)));
  const releases = await getChangelog();
  assert.ok(releases.length > 20);
  assert.equal(new Set(releases.map(release => release.tag)).size, releases.length);
  for (const product of ['desktop', 'watch']) {
    const latest = releases.filter(release => release.product === product && release.latest);
    assert.equal(latest.length, 1);
    assert.equal(latest[0].prerelease, false);
    assert.ok(latest[0].download.url.startsWith('https://github.com/NapNeko/NapCatQQ-Desktop/releases/download/'));
    assert.ok(product === 'desktop' ? latest[0].download.url.endsWith('.msi') : latest[0].download.url.endsWith('x86_64-unknown-linux-musl'));
  }
  assert.ok(releases[0].bodyHtml.includes('自适应布局'));
  assert.ok(!releases[0].bodyHtml.includes('全部提交'));
  assert.ok(releases[0].appendixHtml.includes('完整变更'));
  assert.ok(!releases[0].summary.includes('**'));
});
