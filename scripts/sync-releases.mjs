import { execFileSync } from 'node:child_process';
import { mkdir, writeFile } from 'node:fs/promises';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { dirname, resolve } from 'node:path';

export const repository = 'NapNeko/NapCatQQ-Desktop';
const destination = fileURLToPath(new URL('../src/data/releases.json', import.meta.url));
const versionTag = /^(?:watch-)?v\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/;

function publicURL(value) {
  if (typeof value !== 'string') return null;
  try {
    const url = new URL(value);
    return url.protocol === 'https:' ? url.href : null;
  } catch { return null; }
}

export function normalizeReleases(input) {
  const seen = new Set();
  return input.filter(release => {
    if (release.draft || !versionTag.test(release.tag_name) || !release.published_at || seen.has(release.tag_name)) return false;
    seen.add(release.tag_name);
    return true;
  }).map(release => {
    const url = publicURL(release.html_url);
    if (!url || !url.startsWith(`https://github.com/${repository}/releases/`)) throw new Error(`Invalid release URL for ${release.tag_name}`);
    const date = new Date(release.published_at);
    if (Number.isNaN(date.valueOf())) throw new Error(`Invalid publish date for ${release.tag_name}`);
    return {
      tag: release.tag_name,
      name: typeof release.name === 'string' ? release.name : release.tag_name,
      date: date.toISOString(),
      prerelease: release.prerelease === true,
      url,
      body: typeof release.body === 'string' ? release.body : '',
      assets: (release.assets ?? []).flatMap(asset => {
        const url = publicURL(asset.browser_download_url);
        return url && typeof asset.name === 'string' && Number.isSafeInteger(asset.size) && asset.size >= 0
          ? [{ name: asset.name, url, size: asset.size }] : [];
      }),
    };
  }).sort((a, b) => b.date.localeCompare(a.date) || b.tag.localeCompare(a.tag));
}

export async function synchronize({ dryRun = false, gh = false } = {}) {
  const raw = [];
  for (let page = 1; page <= 20; page++) {
    const endpoint = `repos/${repository}/releases?per_page=100&page=${page}`;
    let releases;
    if (gh) {
      releases = JSON.parse(execFileSync('gh', ['api', endpoint], { encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 }));
    } else {
      const token = process.env.GH_TOKEN || process.env.GITHUB_TOKEN;
      const response = await fetch(`https://api.github.com/${endpoint}`, {
        headers: { Accept: 'application/vnd.github+json', 'User-Agent': 'NapCatQQ-Desktop-Website', ...(token ? { Authorization: `Bearer ${token}` } : {}) },
        signal: AbortSignal.timeout(20_000),
      });
      if (!response.ok) throw new Error(`GitHub Releases request failed (${response.status}); the existing snapshot was not changed.`);
      releases = await response.json();
    }
    if (!Array.isArray(releases)) throw new Error('GitHub returned an unexpected release response.');
    raw.push(...releases);
    if (releases.length < 100) break;
    if (page === 20) throw new Error('Release pagination exceeded the safety limit; the existing snapshot was not changed.');
  }
  const releases = normalizeReleases(raw);
  if (!releases.length) throw new Error('No published version releases were found; the existing snapshot was not changed.');
  const desktop = releases.filter(release => !release.tag.startsWith('watch-'));
  const watch = releases.filter(release => release.tag.startsWith('watch-'));
  console.log(`${dryRun ? 'Plan' : 'Sync'}: ${releases.length} published releases (${desktop.length} Desktop, ${watch.length} ncd-watch) -> src/data/releases.json`);
  console.log(`Newest: ${desktop[0]?.tag ?? 'none'}; ${watch[0]?.tag ?? 'none'}. Drafts, unrelated tags and credentials are excluded; release bodies are preserved.`);
  if (dryRun) return releases;
  await mkdir(dirname(destination), { recursive: true });
  await writeFile(destination, `${JSON.stringify({ schemaVersion: 1, repository, syncedAt: new Date().toISOString(), releases }, null, 2)}\n`);
  return releases;
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  const flags = new Set(process.argv.slice(2));
  for (const flag of flags) if (!['--dry-run', '--gh', '--help'].includes(flag)) throw new Error(`Unknown option: ${flag}`);
  if (flags.has('--help')) console.log('node scripts/sync-releases.mjs [--dry-run] [--gh]\nRefresh the tracked public release snapshot. Builds use the saved snapshot and never require this command.');
  else await synchronize({ dryRun: flags.has('--dry-run'), gh: flags.has('--gh') });
}
