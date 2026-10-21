import assert from 'node:assert/strict';
import test from 'node:test';
import { readFile } from 'node:fs/promises';
import { imageAsset, themeImageSources } from './image-assets.mjs';

test('image URLs include the Pages base for full images and every responsive candidate', () => {
  for (const base of ['/NapCatQQ-Desktop/', 'NapCatQQ-Desktop']) {
    const sources = themeImageSources('components', base);
    for (const source of Object.values(sources)) {
      assert.ok(source.src.startsWith('/NapCatQQ-Desktop/'));
      assert.match(source.src, /\/optimized\/screenshots\/.+\.webp$/);
      assert.ok(source.srcset.includes('1440w'));
      for (const candidate of source.srcset.split(',').filter(Boolean)) assert.ok(candidate.trim().startsWith('/NapCatQQ-Desktop/'));
    }
  }
  assert.ok(imageAsset('media/logo.png').src.startsWith('/'));
  assert.ok(!imageAsset('media/logo.png').src.startsWith('//'));
});

test('relocated SSR helpers still read the prepared project manifest', async () => {
  const code = await readFile(new URL('./image-assets.mjs', import.meta.url), 'utf8');
  const relocated = await import(`data:text/javascript;base64,${Buffer.from(code).toString('base64')}`);
  const sources = relocated.themeImageSources('bot-create', '/NapCatQQ-Desktop/');
  for (const source of Object.values(sources)) {
    assert.match(source.src, /^\/NapCatQQ-Desktop\/optimized\/screenshots\/bot-create-(light|dark)-1440\.[a-f0-9]+\.webp$/);
    assert.ok(source.srcset.includes('1440w'));
  }
  assert.throws(() => relocated.imageAsset('docs/screenshots/missing-light.png'), /Optimized image missing/);
  assert.deepEqual(relocated.imageAsset('media/unmanaged.svg'), { src: '/media/unmanaged.svg', srcset: '' });
});

test('saved document theme, lesson changes, and printing switch one complete image source', async t => {
  const source = name => ({ light: { src: `/site/${name}-light.webp`, srcset: `/site/${name}-light-960.webp 960w, /site/${name}-light.webp 1440w` }, dark: { src: `/site/${name}-dark.webp`, srcset: `/site/${name}-dark-960.webp 960w, /site/${name}-dark.webp 1440w` } });
  const initial = source('components');
  const attributes = new Map([['src', 'data:image/svg+xml,placeholder']]);
  const writes = [];
  const image = {
    dataset: { themeImage: JSON.stringify(initial) },
    getAttribute: name => attributes.get(name) ?? null,
    setAttribute(name, value) { attributes.set(name, value); writes.push([name, value]); },
  };
  const root = { dataset: { theme: 'dark' } };
  const events = new Map();
  let observeTheme;
  const originals = Object.fromEntries(['document', 'window', 'MutationObserver'].map(key => [key, Object.getOwnPropertyDescriptor(globalThis, key)]));
  t.after(() => {
    for (const [key, descriptor] of Object.entries(originals)) {
      if (descriptor) Object.defineProperty(globalThis, key, descriptor);
      else delete globalThis[key];
    }
  });
  globalThis.document = { documentElement: root, querySelectorAll: () => [image] };
  globalThis.window = { addEventListener: (name, listener) => events.set(name, listener) };
  globalThis.MutationObserver = class { constructor(callback) { observeTheme = callback; } observe() {} };
  const api = await import('../src/scripts/theme-images.ts');
  assert.equal(attributes.get('src'), initial.dark.src);
  assert.equal(attributes.get('srcset'), initial.dark.srcset);
  assert.ok(writes.every(([, value]) => !value.includes('-light')));

  root.dataset.theme = 'light'; observeTheme();
  assert.equal(attributes.get('src'), initial.light.src);
  assert.equal(attributes.get('srcset'), initial.light.srcset);
  root.dataset.theme = 'dark'; observeTheme();
  const next = source('debug');
  api.setThemeImage(image, next);
  assert.equal(attributes.get('srcset'), next.dark.srcset);
  assert.equal(api.getThemeImageSource(image).src, next.dark.src);
  events.get('beforeprint')();
  assert.equal(attributes.get('src'), next.light.src);
  events.get('afterprint')();
  assert.equal(attributes.get('src'), next.dark.src);
});
