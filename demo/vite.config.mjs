import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const demoRoot = fileURLToPath(new URL('./', import.meta.url));
const siteRoot = path.resolve(demoRoot, '..');
const cacheRoot = path.join(siteRoot, '.cache/desktop');
const desktopRoot = path.join(cacheRoot, 'src-ui');
const dependencyRoot = process.env.DESKTOP_DEMO_NODE_MODULES
  ? path.dirname(path.resolve(process.env.DESKTOP_DEMO_NODE_MODULES)) : cacheRoot;
async function load(name) {
  const packageRoot = path.join(dependencyRoot, 'node_modules', name);
  const metadata = JSON.parse(await readFile(path.join(packageRoot, 'package.json'), 'utf8'));
  const rootExport = metadata.exports?.['.'] ?? metadata.exports;
  const imported = typeof rootExport === 'string' ? rootExport : rootExport?.import ?? rootExport?.default;
  const entry = typeof imported === 'string' ? imported : imported?.default;
  if (typeof entry !== 'string' || !entry.startsWith('./')) {
    throw new Error(`No supported ESM runtime export for ${name}@${metadata.version}.`);
  }
  return import(pathToFileURL(path.resolve(packageRoot, entry)).href);
}
const react = (await load('@vitejs/plugin-react')).default;
const tailwindcss = (await load('@tailwindcss/vite')).default;
if (typeof react !== 'function' || typeof tailwindcss !== 'function') {
  throw new Error('The pinned Vite plugins must expose callable ESM defaults.');
}
const canonical = value => {
  const normalized = path.resolve(value).replaceAll('\\', '/');
  return process.platform === 'win32' ? normalized.toLowerCase() : normalized;
};
const clean = id => id.split('?')[0];
const mockTargets = new Map([
  [canonical(path.join(desktopRoot, 'core/ipc/mock/bot.mock')), path.join(demoRoot, 'bot.mock.ts')],
  [canonical(path.join(desktopRoot, 'core/ipc/mock/events.mock')), path.join(demoRoot, 'events.mock.ts')],
]);
const avatarPath = canonical(path.join(desktopRoot, 'modules/bot/BotAvatar.tsx'));
const preferencesPath = canonical(path.join(desktopRoot, 'core/domain/settings/preferencesStore.ts'));
const connectionsPath = canonical(path.join(desktopRoot, 'modules/bot/config/next/ConnectionsTab.tsx'));

function replaceOnce(code, before, after, name) {
  const index = code.indexOf(before);
  if (index < 0 || code.indexOf(before, index + before.length) >= 0) {
    throw new Error(`Pinned Desktop demo adaptation no longer matches exactly once: ${name}`);
  }
  return code.slice(0, index) + after + code.slice(index + before.length);
}

function desktopDemoAdapter() {
  return {
    name: 'desktop-docs-demo',
    enforce: 'pre',
    async resolveId(id, importer) {
      const request = clean(id);
      let absolute;
      if (path.isAbsolute(request)) absolute = request;
      else if (request.startsWith('.') && importer) absolute = path.resolve(path.dirname(clean(importer)), request);
      if (absolute) {
        const replacement = mockTargets.get(canonical(absolute).replace(/\.ts$/, ''));
        if (replacement) return replacement.replaceAll('\\', '/');
      }
      // Both the entry and exported source resolve from one dependency tree, including explicit read-only reuse.
      if (importer && [demoRoot, desktopRoot].some(root => canonical(clean(importer)).startsWith(`${canonical(root)}/`))
        && !request.startsWith('.') && !request.startsWith('/') && !request.includes(':') && !request.startsWith('\0')) {
        // Vite 5 falls back to root when its importer does not exist on disk.
        return this.resolve(id, path.join(dependencyRoot, 'package.json'), { skipSelf: true });
      }
      return null;
    },
    transform(code, id) {
      const target = canonical(clean(id));
      if (target === avatarPath) {
        return { code: replaceOnce(code, 'const showImg = numericQQ && !failed;', 'const showImg = false;', 'disable external QQ avatars'), map: null };
      }
      if (target === preferencesPath) {
        return {
          code: replaceOnce(code, "const STORAGE_KEY = 'ncd:preferences:v1';", "const STORAGE_KEY = 'ncd:docs-demo:preferences:v1';", 'isolate demo preferences'),
          map: null,
        };
      }
      if (target === connectionsPath) {
        // The pinned frontend mounts the dialog body one effect after its open state.
        return {
          code: replaceOnce(code, 'open={editing !== null}', 'open={editing !== null && editingMount !== null}', 'mount connection dialog title before opening'),
          map: null,
        };
      }
      return null;
    },
    buildEnd(error) {
      if (error) return;
      const ids = [...this.getModuleIds()];
      for (const file of ['bot.mock.ts', 'events.mock.ts']) {
        const target = canonical(path.join(demoRoot, file));
        const matches = ids.filter(id => !id.startsWith('\0') && canonical(clean(id)) === target);
        if (matches.length !== 1) this.error(`Expected one shared demo module for ${file}, received ${matches.length}.`);
      }
    },
  };
}

export default {
  root: demoRoot,
  base: './',
  envDir: demoRoot,
  cacheDir: path.join(cacheRoot, '.vite'),
  publicDir: path.join(desktopRoot, 'public'),
  plugins: [desktopDemoAdapter(), react(), tailwindcss()],
  resolve: {
    alias: [
      // React's Vite plugin dedupes from demoRoot; these aliases keep its runtime in the pinned tree.
      { find: 'react', replacement: path.join(dependencyRoot, 'node_modules/react') },
      { find: 'react-dom', replacement: path.join(dependencyRoot, 'node_modules/react-dom') },
      { find: '@desktop', replacement: desktopRoot },
      { find: '@demo', replacement: demoRoot },
      { find: '@', replacement: desktopRoot },
    ],
  },
  define: {
    'import.meta.env.VITE_SKIP_COMPONENTS_WARMUP': JSON.stringify('1'),
    'import.meta.env.VITE_UI_NEXT': JSON.stringify('1'),
  },
  build: {
    outDir: path.join(siteRoot, 'public/demo'),
    emptyOutDir: true,
    target: 'es2022',
    sourcemap: false,
  },
};
