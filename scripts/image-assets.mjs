import { readFileSync, statSync } from 'node:fs';
import { resolve } from 'node:path';

// Astro relocates this module into its SSR bundle while keeping the project cwd.
const manifestPath = resolve('public/optimized/manifest.json');
let cached;
let modified;

function assets() {
  try {
    const timestamp = statSync(manifestPath).mtimeMs;
    if (timestamp !== modified) {
      cached = JSON.parse(readFileSync(manifestPath, 'utf8')).assets;
      modified = timestamp;
    }
    return cached;
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
    throw new Error(`Image manifest not found at ${manifestPath}. Run pnpm images:prepare before rendering.`, { cause: error });
  }
}

export const emptyImage = 'data:image/svg+xml,%3Csvg xmlns="http://www.w3.org/2000/svg" width="1" height="1"/%3E';

export function imageAsset(source, base = '/') {
  const prefix = `/${base.replace(/^\/+|\/+$/g, '')}`.replace(/\/$/, '') + '/';
  const entry = assets()[source];
  if (!entry) {
    if (/^docs\/screenshots\/.+-(light|dark)\.png$/.test(source) || /^media\/(logo|favicon)\.png$/.test(source)) {
      throw new Error(`Optimized image missing for ${source}. Run pnpm images:prepare after adding or changing source images.`);
    }
    return { src: prefix + source, srcset: '' };
  }
  return {
    src: prefix + entry.src,
    srcset: entry.variants.map(image => `${prefix}${image.path} ${image.width}w`).join(', '),
  };
}

export function themeImageSources(poster, base = '/') {
  return {
    light: imageAsset(`docs/screenshots/${poster}-light.png`, base),
    dark: imageAsset(`docs/screenshots/${poster}-dark.png`, base),
  };
}
