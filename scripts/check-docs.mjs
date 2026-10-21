import { readdir, readFile, stat } from 'node:fs/promises';
import path from 'node:path';
import { createHash } from 'node:crypto';

const root = path.resolve(import.meta.dirname, '..');
const output = path.join(root, 'dist');
const base = `/${(process.env.SITE_BASE || '/').replace(/^\/+|\/+$/g, '')}`.replace(/\/?$/, '/');
const errors = [];
const assert = (condition, message) => { if (!condition) errors.push(message); };
async function files(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  return (await Promise.all(entries.map(entry => entry.isDirectory() ? files(path.join(directory, entry.name)) : path.join(directory, entry.name)))).flat();
}
const exists = async file => { try { return (await stat(file)).isFile(); } catch { return false; } };
const decode = value => value.replaceAll('&amp;', '&').replaceAll('&quot;', '"').replaceAll('&#39;', "'");
const htmlFiles = (await files(output)).filter(file => file.endsWith('.html'));
const cache = new Map(await Promise.all(htmlFiles.map(async file => [file, await readFile(file, 'utf8')])));
let links = 0, images = 0;
for (const [file, html] of cache) {
  for (const tag of html.matchAll(/<(?:a|img|script|link)\b[^>]*>/g)) {
    const match = tag[0].match(/\b(?:href|src)="([^"]+)"/);
    if (!match) continue;
    const url = decode(match[1]);
    if (!url || /^(?:[a-z][a-z\d+.-]*:|\/\/)/i.test(url)) continue;
    if (url.startsWith('/')) assert(url.startsWith(base), `${path.relative(output, file)}: URL omits SITE_BASE: ${url}`);
    const relativeFile = path.relative(output, file).split(path.sep).join('/');
    const currentURL = `https://docs.invalid${base}${relativeFile}`;
    const parsed = new URL(url, currentURL);
    const pathname = decodeURIComponent(parsed.pathname);
    const inside = pathname.startsWith(base) ? pathname.slice(base.length) : pathname.slice(1);
    let target = path.join(output, inside);
    if (pathname.endsWith('/')) target = path.join(target, 'index.html');
    if (!(await exists(target)) && await exists(`${target}/index.html`)) target = `${target}/index.html`;
    assert(await exists(target), `${path.relative(output, file)}: missing local resource ${url}`);
    if (tag[0].startsWith('<img')) {
      images++;
      assert(/\balt="[^"]*"/.test(tag[0]), `${path.relative(output, file)}: image has no alt: ${url}`);
    } else links++;
    if (parsed.hash && cache.has(target)) {
      const id = decodeURIComponent(parsed.hash.slice(1));
      const ids = [...cache.get(target).matchAll(/\bid="([^"]+)"/g)].map(match => decode(match[1]));
      assert(ids.includes(id), `${path.relative(output, file)}: missing anchor ${url}`);
    }
  }
}
const contentDir = path.join(root, 'src/content/docs');
const sources = new Map(await Promise.all((await files(contentDir)).filter(file => file.endsWith('.md')).map(async file => [path.basename(file, '.md'), await readFile(file, 'utf8')])));
const screenshotFiles = (await files(path.join(root, 'public/docs/screenshots'))).filter(file => /-(light|dark)\.png$/.test(file));
for (const file of screenshotFiles) {
  const pair = file.replace(/-(light|dark)\.png$/, file.endsWith('-light.png') ? '-dark.png' : '-light.png');
  assert(await exists(pair), `Screenshot is missing paired theme: ${path.basename(file)}`);
  const buffer = await readFile(file);
  assert(buffer.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10])), `Invalid PNG: ${path.basename(file)}`);
  if (buffer.length >= 24) assert(buffer.readUInt32BE(16) === 1440 && buffer.readUInt32BE(20) === 1000, `Unexpected screenshot dimensions: ${path.basename(file)}`);
}
const recordedImages = new Set();
for (const file of (await files(path.join(root, 'public/docs/screenshots'))).filter(file => file.endsWith('-manifest.json'))) {
  const manifest = JSON.parse(await readFile(file, 'utf8'));
  for (const entry of manifest.screenshots || manifest.screens || []) {
    recordedImages.add(entry.file);
    const image = path.join(root, 'public/docs/screenshots', entry.file);
    assert(await exists(image), `${path.basename(file)}: missing capture ${entry.file}`);
    if (entry.sha256 && await exists(image)) assert(createHash('sha256').update(await readFile(image)).digest('hex') === entry.sha256, `${path.basename(file)}: changed image hash ${entry.file}`);
  }
}
for (const image of screenshotFiles) assert(recordedImages.has(path.basename(image)), `Screenshot has no provenance record: ${path.basename(image)}`);
for (const [slug, text] of sources) {
  assert(!/<!--\s*(?:TODO|截图|待补)|\bTODO\b|\bFIXME\b/.test(text), `${slug}: unfinished authoring marker`);
  assert(cache.has(path.join(output, 'docs', slug === 'index' ? '' : slug, 'index.html')), `${slug}: page not built`);
}
const inventoryPath = path.join(root, 'scripts/docs-coverage.json');
if (await exists(inventoryPath)) {
  const inventory = JSON.parse(await readFile(inventoryPath, 'utf8'));
  const ids = new Set();
  let covered = 0;
  for (const capability of inventory.capabilities) {
    assert(!ids.has(capability.id), `Duplicate capability ID: ${capability.id}`); ids.add(capability.id);
    for (const slug of capability.docs) assert(sources.has(slug), `${capability.id}: missing document ${slug}`);
    if (capability.status === 'covered') {
      covered++;
      const text = capability.docs.map(slug => sources.get(slug) || '').join('\n');
      for (const keyword of capability.keywords || []) assert(text.includes(keyword), `${capability.id}: documented keyword missing: ${keyword}`);
    }
  }
  const percentage = covered / inventory.capabilities.length * 100;
  assert(percentage > 90, `Documented capability coverage ${percentage.toFixed(1)}% must exceed 90%`);
  console.log(`Capability inventory: ${covered}/${inventory.capabilities.length} (${percentage.toFixed(1)}%) manually audited; keyword presence checked.`);
}
if (errors.length) {
  for (const error of errors) console.error(`✗ ${error}`);
  process.exitCode = 1;
} else {
  console.log(`✓ ${sources.size} docs; ${htmlFiles.length} HTML pages; ${links} local links/assets; ${images} image references; ${screenshotFiles.length / 2} screenshot pairs.`);
  console.log(`✓ Local resources, anchors, paired themes, authored markers and SITE_BASE (${base}) checked.`);
}
