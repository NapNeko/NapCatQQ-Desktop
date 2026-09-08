// 市场条目 + 已装扫描 → 卡片列表。字段名跟 ts-rs 生成走（type / author / allowBuild）。

import type {
    AppPluginAction,
    DeploymentTaskStatus,
    KarinPluginInstalled,
    KarinPluginKind,
    KarinPluginMarketEntry,
} from '../../../../core/ipc/types';

export type KarinPluginKindFilter = 'all' | KarinPluginKind;

export type VisiblePlugin = {
    name: string;
    kind: KarinPluginKind;
    description: string;
    authorName: string;
    installed: boolean;
    enabled: boolean;
    version?: string;
    timeLabel: string | null;
};

export function parseKarinPluginTime(time: string): number | null {
    const t = time.trim();
    if (!t) return null;
    const iso = t.includes('T') ? t : t.replace(' ', 'T');
    const ts = Date.parse(iso);
    return Number.isNaN(ts) ? null : ts;
}

/** 与 Bot 卡同一套阈值，避免 modules/bot → apps 依赖。 */
function formatRelativeTime(iso: string): string | null {
    const ts = Date.parse(iso);
    if (Number.isNaN(ts)) return null;
    const diffSec = Math.max(0, Math.floor((Date.now() - ts) / 1000));
    if (diffSec < 5) return '刚刚';
    if (diffSec < 60) return `${diffSec} 秒前`;
    const min = Math.floor(diffSec / 60);
    if (min < 60) return `${min} 分钟前`;
    const hr = Math.floor(min / 60);
    if (hr < 24) return `${hr} 小时前`;
    const day = Math.floor(hr / 24);
    if (day < 7) return `${day} 天前`;
    const week = Math.floor(day / 7);
    if (week < 4) return `${week} 周前`;
    const month = Math.floor(day / 30);
    if (month < 12) return `${month} 个月前`;
    const year = Math.floor(day / 365);
    return `${year} 年前`;
}

export function pluginCatalogErrorCopy(raw: string): { title: string; detail: string | null } {
    const stripped = raw
        .replace(/^写入应用端配置失败:\s*/g, '')
        .replace(/^拉取插件目录失败:\s*/g, '')
        .replace(/^读取插件目录失败:\s*/g, '')
        .trim();
    if (/error sending request|timed out|connection refused|dns|network|proxy/i.test(raw)) {
        return { title: '无法连接官方插件目录', detail: '检查网络或代理后重试' };
    }
    if (/HTTP\s+[45]\d\d/.test(raw)) {
        return { title: '官方插件目录暂时不可用', detail: '稍后重试' };
    }
    if (!stripped) return { title: '插件目录加载失败', detail: null };
    return {
        title: '插件目录加载失败',
        detail: stripped.length > 120 ? `${stripped.slice(0, 120)}…` : stripped,
    };
}

export function appFileBasename(url: string): string {
    const path = (url.split('?')[0] ?? '').split('#')[0] ?? '';
    return path.split('/').pop() ?? '';
}

function matchesQuery(row: Pick<VisiblePlugin, 'name' | 'description' | 'authorName'>, query: string): boolean {
    const q = query.trim().toLowerCase();
    if (!q) return true;
    return (
        row.name.toLowerCase().includes(q) ||
        row.description.toLowerCase().includes(q) ||
        row.authorName.toLowerCase().includes(q)
    );
}

function marketCoveredNames(entries: readonly KarinPluginMarketEntry[]): Set<string> {
    const covered = new Set<string>();
    for (const entry of entries) {
        covered.add(entry.name);
        if (entry.type === 'app') {
            for (const file of entry.files) {
                const base = appFileBasename(file.url);
                if (base) covered.add(base);
            }
        }
    }
    return covered;
}

function marketInstallState(
    entry: KarinPluginMarketEntry,
    installed: readonly KarinPluginInstalled[],
): { installed: boolean; enabled: boolean; version?: string } {
    const exact = installed.find((i) => i.name === entry.name);
    if (exact) {
        return { installed: true, enabled: exact.enabled, version: exact.version };
    }
    if (entry.type === 'app') {
        const names = entry.files.map((f) => appFileBasename(f.url)).filter(Boolean);
        const hits = installed.filter((i) => names.includes(i.name));
        if (hits.length > 0) {
            return { installed: true, enabled: hits.every((h) => h.enabled) };
        }
    }
    return { installed: false, enabled: true };
}

function timeLabelFor(entry: KarinPluginMarketEntry, installed: boolean, version?: string): string | null {
    if (installed) return version ? `v${version}` : '已装';
    const ts = parseKarinPluginTime(entry.time);
    if (ts == null) return null;
    return formatRelativeTime(new Date(ts).toISOString());
}

export type PluginTaskHint = {
    pluginName: string;
    action: AppPluginAction;
    status: DeploymentTaskStatus;
    atMs: number;
};

/** 扫描还没跟上时，用最近一条终态任务补「已装 / 已卸」。 */
export function overlayInstalledFromTasks(
    installed: readonly KarinPluginInstalled[],
    hints: readonly PluginTaskHint[],
): KarinPluginInstalled[] {
    const latest = new Map<string, PluginTaskHint>();
    for (const hint of hints) {
        const prev = latest.get(hint.pluginName);
        if (!prev || hint.atMs >= prev.atMs) latest.set(hint.pluginName, hint);
    }
    let next = [...installed];
    for (const hint of latest.values()) {
        if (hint.status !== 'success') continue;
        if (hint.action === 'uninstall') {
            next = next.filter((item) => item.name !== hint.pluginName);
            continue;
        }
        if (!next.some((item) => item.name === hint.pluginName)) {
            next.push({
                name: hint.pluginName,
                kind: 'npm',
                enabled: true,
            });
        }
    }
    return next;
}

export function filterKarinPlugins(
    entries: readonly KarinPluginMarketEntry[],
    installed: readonly KarinPluginInstalled[],
    query: string,
    kindFilter: KarinPluginKindFilter,
): VisiblePlugin[] {
    const rows: VisiblePlugin[] = [];
    for (const entry of entries) {
        if (kindFilter !== 'all' && entry.type !== kindFilter) continue;
        const state = marketInstallState(entry, installed);
        const row: VisiblePlugin = {
            name: entry.name,
            kind: entry.type,
            description: entry.description,
            authorName: entry.author[0]?.name ?? '',
            installed: state.installed,
            enabled: state.enabled,
            version: state.version,
            timeLabel: timeLabelFor(entry, state.installed, state.version),
        };
        if (matchesQuery(row, query)) rows.push(row);
    }

    const covered = marketCoveredNames(entries);
    for (const item of installed) {
        if (covered.has(item.name)) continue;
        if (kindFilter !== 'all' && item.kind !== kindFilter) continue;
        const row: VisiblePlugin = {
            name: item.name,
            kind: item.kind,
            description: '',
            authorName: '',
            installed: true,
            enabled: item.enabled,
            version: item.version,
            timeLabel: item.version ? `v${item.version}` : '已装',
        };
        if (matchesQuery(row, query)) rows.push(row);
    }
    return rows;
}
