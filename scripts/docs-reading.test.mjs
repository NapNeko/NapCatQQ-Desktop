import assert from 'node:assert/strict';
import test from 'node:test';
import { readingOutline, readingProgress } from '../src/lib/reading-outline.ts';

const heading = (text, depth = 2) => ({ text, depth, slug: text });

test('steps exclude prerequisites and appendices, while subheadings keep their parent step', () => {
  const outline = readingOutline([
    heading('开始前'), heading('1. 创建 Bot'), heading('填写账号', 3),
    heading('2. 完成登录'), heading('无法登录时'),
  ]);
  assert.deepEqual(outline.map(item => item.position), ['开始前', '第 1 / 2 步', '第 1 / 2 步', '第 2 / 2 步', '补充说明']);
  assert.equal(outline[1].title, '创建 Bot');
  assert.equal(outline[2].section, '1. 创建 Bot');
  assert.equal(outline[4].marker, '·');
});

test('reference articles and discontinuous numbering use chapters instead of claiming a workflow', () => {
  assert.deepEqual(readingOutline([heading('QQ 离线'), heading('检查日志', 3), heading('SSH 连不上')]).map(item => item.position), ['章节 1 / 2', '章节 1 / 2', '章节 2 / 2']);
  assert.deepEqual(readingOutline([heading('1. 独立项目'), heading('3. 另一个项目')]).map(item => item.position), ['章节 1 / 2', '章节 2 / 2']);
  assert.deepEqual(readingOutline([]), []);
});

test('reading progress is clamped to the article, excluding the page header, demo and footer', () => {
  assert.equal(readingProgress(0, 1000, 3000, 800, 100), 0);
  assert.equal(readingProgress(900, 1000, 3000, 800, 100), 0);
  assert.equal(readingProgress(1550, 1000, 3000, 800, 100), 50);
  assert.equal(readingProgress(2200, 1000, 3000, 800, 100), 100);
  assert.equal(readingProgress(2800, 1000, 3000, 800, 100), 100);
  assert.equal(readingProgress(0, 300, 600, 800, 100), 0);
  assert.equal(readingProgress(200, 300, 600, 800, 100), 100);
});
