import { satteri } from '@astrojs/markdown-satteri';
import snapshot from '../data/releases.json' with { type: 'json' };

export type ReleaseAsset = { name: string; url: string; size: number };
export type ChangelogRelease = {
  tag: string;
  version: string;
  product: 'desktop' | 'watch';
  name: string;
  date: string;
  prerelease: boolean;
  url: string;
  bodyHtml: string;
  appendixHtml: string;
  summary: string;
  download?: { url: string; label: string; size?: number };
  assets: ReleaseAsset[];
  latest: boolean;
};

type SavedRelease = (typeof snapshot.releases)[number];
type MarkdownNode = {
  type: string;
  tagName?: string;
  properties?: Record<string, unknown>;
  children?: MarkdownNode[];
  value?: string;
};

const repository = 'https://github.com/NapNeko/NapCatQQ-Desktop';
const allowedTags = new Set('a abbr b blockquote br code del details div dl dt dd em h1 h2 h3 h4 h5 h6 hr i img input kbd li mark ol p pre s small span strong sub summary sup table tbody td th thead tr ul'.split(' '));
const plain = (value: string) => value.replace(/!?\[([^\]]*)\]\([^)]*\)/g, '$1').replace(/[`*_~]/g, '').replace(/\s+/g, ' ').trim();
const slug = (value: string) => value.toLowerCase().replace(/[^\p{L}\p{N}]+/gu, '-').replace(/^-|-$/g, '');
const text = (node: MarkdownNode): string => node.type === 'text' ? node.value ?? '' : (node.children ?? []).map(text).join('');
export const changelogSyncedAt = snapshot.syncedAt;

export function splitReleaseBody(source: string) {
  const appendix: string[] = [];
  let body = source.replace(/<!--\s*ncd-release-notes:\s*download\s*-->[\s\S]*?<!--\s*ncd-release-notes:\s*\/download\s*-->/g, section => {
    appendix.push(section);
    return '';
  });
  const footer = body.search(/<!--\s*ncd-release-notes:\s*footer\s*-->/);
  if (footer >= 0) {
    appendix.push(body.slice(footer));
    body = body.slice(0, footer);
  }
  return { body: body.trim(), appendix: appendix.join('\n\n').trim() };
}

export function releaseSummary(source: string, fallback: string) {
  const withoutComments = source.replace(/<!--[\s\S]*?-->/g, '').replace(/```[\s\S]*?```/g, '');
  const paragraph = withoutComments.split(/\n\s*\n/).map(value => value.trim()).find(value =>
    value && !/^(?:#{1,6}\s|[-*+]\s|\d+[.)]\s|\||<|!\[|---+$)/.test(value));
  return paragraph ? plain(paragraph.replace(/^>\s?/gm, '')) : fallback;
}

function safeLink(value: unknown, tag: string, prefix: string, image = false): string | undefined {
  if (typeof value !== 'string' || /[\u0000-\u0020\u007f]/.test(value)) return undefined;
  if (value.startsWith('#') && !image) return `#${prefix}-${slug(decodeURIComponentSafe(value.slice(1)))}`;
  try {
    let url: URL;
    if (value.startsWith('/docs/')) url = new URL(`${repository}/blob/${encodeURIComponent(tag)}${value}`);
    else if (value.startsWith('/')) url = new URL(value, repository);
    else url = new URL(value, `${repository}/blob/${encodeURIComponent(tag)}/`);
    return (url.protocol === 'https:' || url.protocol === 'http:' || (!image && url.protocol === 'mailto:')) ? url.href : undefined;
  } catch { return undefined; }
}

function decodeURIComponentSafe(value: string) {
  try { return decodeURIComponent(value); } catch { return value; }
}

function sanitizeNode(node: MarkdownNode, tag: string, prefix: string, headings: Map<string, number>): MarkdownNode | null {
  if (node.type === 'text') return { type: 'text', value: node.value ?? '' };
  if (node.type !== 'root' && (node.type !== 'element' || !allowedTags.has(node.tagName ?? ''))) return null;
  const children = (node.children ?? []).flatMap(child => {
    const clean = sanitizeNode(child, tag, prefix, headings);
    return clean ? [clean] : [];
  });
  if (node.type === 'root') return { type: 'root', children };
  const properties: Record<string, unknown> = {};
  const original = node.properties ?? {};
  const tagName = node.tagName!;
  if (tagName === 'a') {
    const href = safeLink(original.href, tag, prefix);
    if (href) properties.href = href;
    properties.rel = ['noreferrer'];
  }
  if (tagName === 'img') {
    const src = safeLink(original.src, tag, prefix, true);
    if (!src) return null;
    properties.src = src;
    properties.alt = typeof original.alt === 'string' ? original.alt : '';
    properties.loading = 'lazy';
    properties.decoding = 'async';
  }
  if (typeof original.title === 'string') properties.title = original.title;
  if (tagName === 'ol' && Number.isSafeInteger(original.start)) properties.start = original.start;
  if (tagName === 'input') {
    if (original.type !== 'checkbox') return null;
    properties.type = 'checkbox';
    properties.disabled = true;
    if (original.checked) properties.checked = true;
  }
  if (/^h[1-6]$/.test(tagName)) {
    const label = text(node);
    const heading = slug(label);
    const previous = headings.get(heading) ?? 0;
    headings.set(heading, previous + 1);
    properties.id = `${prefix}-${heading}${previous ? `-${previous}` : ''}`;
    properties['data-change-kind'] = /修复|修正/.test(label) ? 'fix' : /改进|优化/.test(label) ? 'improve' : /新增|新功能/.test(label) ? 'new' : /注意|安装|升级|兼容/.test(label) ? 'notice' : 'general';
  }
  if (tagName === 'code' && Array.isArray(original.className)) properties.className = original.className.filter(value => typeof value === 'string' && /^language-[\w+-]+$/.test(value));
  return { type: 'element', tagName, properties, children };
}

export async function renderReleaseMarkdown(source: string, tag: string, suffix = '') {
  const prefix = suffix ? `${tag}-${suffix}` : tag;
  const processor = satteri({
    features: { smartPunctuation: false },
    hastPlugins: [{ name: 'safe-release-content', after(root, context) {
      // Release bodies are public source content, so only inert prose reaches set:html.
      context.replaceNode(root, sanitizeNode(root, tag, prefix, new Map()) as typeof root);
    } }],
  });
  const renderer = await processor.createRenderer({ syntaxHighlight: false });
  return (await renderer.render(source)).code;
}

function primaryDownload(release: SavedRelease, product: ChangelogRelease['product']) {
  const asset = product === 'watch'
    ? release.assets.find(asset => /x86_64-unknown-linux-musl$/.test(asset.name))
    : release.assets.find(asset => /\.msi$/i.test(asset.name) && asset.name.includes(release.tag.slice(1)))
      ?? release.assets.find(asset => /\.msi$/i.test(asset.name))
      ?? release.assets.find(asset => /\.exe$/i.test(asset.name) && !/debug/i.test(asset.name));
  return asset ? { url: asset.url, label: product === 'watch' ? 'Linux x86_64' : /\.msi$/i.test(asset.name) ? '下载 Windows 安装包' : '下载 Windows 版', size: asset.size } : undefined;
}

let rendered: Promise<ChangelogRelease[]> | undefined;
export function getChangelog(): Promise<ChangelogRelease[]> {
  rendered ??= (async () => {
    const latest = new Set<string>();
    const releases: ChangelogRelease[] = [];
    for (const release of snapshot.releases) {
      const product = release.tag.startsWith('watch-') ? 'watch' : 'desktop';
      const parts = splitReleaseBody(release.body);
      const current = !release.prerelease && !latest.has(product);
      if (current) latest.add(product);
      releases.push({
        tag: release.tag, version: release.tag.replace(/^(?:watch-)?v/, ''), product,
        name: release.name, date: release.date, prerelease: release.prerelease, url: release.url,
        summary: releaseSummary(parts.body, release.name),
        bodyHtml: await renderReleaseMarkdown(parts.body, release.tag),
        appendixHtml: parts.appendix ? await renderReleaseMarkdown(parts.appendix, release.tag, 'appendix') : '',
        download: primaryDownload(release, product), assets: release.assets, latest: current,
      });
    }
    return releases;
  })();
  return rendered;
}
