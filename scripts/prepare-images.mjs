import { createHash } from 'node:crypto';
import { mkdir, readFile, readdir, stat, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const publicRoot = join(root, 'public');
const require = createRequire(import.meta.url);
const sharp = createRequire(require.resolve('astro/package.json'))('sharp');
const settings = { lossless: true, effort: 6 };

async function sources() {
  const screenshots = (await readdir(join(publicRoot, 'docs/screenshots')))
    .filter(name => /-(light|dark)\.png$/.test(name)).sort();
  return [...screenshots.map(name => `docs/screenshots/${name}`), 'media/logo.png'];
}

async function variant(source, input, width, format = 'webp') {
  const options = { width, format, settings, sharp: sharp.versions.sharp, vips: sharp.versions.vips };
  const hash = createHash('sha256').update(input).update(JSON.stringify(options)).digest('hex').slice(0, 16);
  const name = source.split('/').at(-1).replace(/\.png$/, '');
  const path = `optimized/${source.startsWith('docs/') ? 'screenshots/' : ''}${name}-${width}.${hash}.${format}`;
  const destination = join(publicRoot, path);
  try {
    const metadata = await sharp(destination).metadata();
    return { path, width: metadata.width, height: metadata.height, bytes: (await stat(destination)).size };
  } catch (error) {
    if (error.code && error.code !== 'ENOENT') throw error;
  }
  const pipeline = sharp(input).resize({ width, withoutEnlargement: true });
  const { data, info } = await (format === 'png' ? pipeline.png({ compressionLevel: 9 }) : pipeline.webp(settings)).toBuffer({ resolveWithObject: true });
  return { path, width: info.width, height: info.height, bytes: info.size, data };
}

async function saveVariant(image) {
  if (image.data) {
    const destination = join(publicRoot, image.path);
    await mkdir(dirname(destination), { recursive: true });
    await writeFile(destination, image.data);
  }
  const { data, ...record } = image;
  return record;
}

export async function prepareImages({ dryRun = false } = {}) {
  const inputs = await sources();
  if (dryRun) {
    for (const source of inputs) console.log(`${source} -> optimized/${source.startsWith('docs/') ? 'screenshots/' : ''}${source.split('/').at(-1).replace(/\.png$/, '')}-{width}.{content-hash}.{format}`);
    console.log(`${inputs.length} source PNGs; ${(inputs.length - 1) * 2 + 2} candidate derivatives plus manifest.json. Screenshots: full 1440px lossless WebP, 960px only when smaller. Logo: 128px WebP and 64px PNG favicon. Originals are preserved.`);
    return;
  }

  const assets = {};
  let originalBytes = 0;
  let fullBytes = 0;
  let derivativeBytes = 0;
  for (const source of inputs) {
    const input = await readFile(join(publicRoot, source));
    originalBytes += input.length;
    const logo = source === 'media/logo.png';
    const full = await variant(source, input, logo ? 128 : 1440);
    const variants = [];
    if (!logo && full.width > 960) {
      const compact = await variant(source, input, 960);
      // Resampling can cost more than preserving sharp UI edges at native size.
      if (compact.bytes < full.bytes) variants.push(await saveVariant(compact));
    }
    variants.push(await saveVariant(full));
    assets[source] = { src: full.path, width: full.width, height: full.height, variants };
    fullBytes += full.bytes;
    derivativeBytes += variants.reduce((sum, image) => sum + image.bytes, 0);
    if (logo) {
      const favicon = await saveVariant(await variant(source, input, 64, 'png'));
      assets['media/favicon.png'] = { src: favicon.path, width: favicon.width, height: favicon.height, variants: [favicon] };
      derivativeBytes += favicon.bytes;
    }
  }
  const manifest = { version: 1, assets };
  const destination = join(publicRoot, 'optimized/manifest.json');
  const contents = `${JSON.stringify(manifest, null, 2)}\n`;
  let previous;
  try { previous = await readFile(destination, 'utf8'); } catch (error) { if (error.code !== 'ENOENT') throw error; }
  if (previous !== contents) await writeFile(destination, contents);
  console.log(`[images] ${inputs.length} sources: ${(originalBytes / 1024).toFixed(0)} KiB PNG -> ${(fullBytes / 1024).toFixed(0)} KiB full-size WebP (${(100 - fullBytes / originalBytes * 100).toFixed(1)}% smaller); ${(derivativeBytes / 1024).toFixed(0)} KiB including responsive variants and favicon.`);
  return manifest;
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  await prepareImages({ dryRun: process.argv.includes('--dry-run') });
}
