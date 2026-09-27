// 市场条目 + 已装扫描 → 卡片列表。字段名跟 ts-rs 生成走（type / author / allowBuild）。
// 时间、搜索、任务补显示和应用端商店一套，在 pluginCatalog。

import {
    catalogTimeLabel,
    installedLabel,
    matchesCatalogQuery,
    overlayInstalledFromTasks,
    type PluginTaskHint,
} from './pluginCatalog';
import type { KarinPluginInstalled, KarinPluginKind, KarinPluginMarketEntry } from '../../ipc/types';

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

/** 拉目录失败时标题里的叫法 */
export const KARIN_CATALOG = '插件目录';

export function appFileBasename(url: string): string {
    const path = (url.split('?')[0] ?? '').split('#')[0] ?? '';
    return path.split('/').pop() ?? '';
}

function matchesQuery(row: VisiblePlugin, query: string): boolean {
    return matchesCatalogQuery(query, [row.name, row.description, row.authorName]);
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

/** 扫描还没跟上时，用最近一条终态任务补「已装 / 已卸」。 */
export function overlayKarinInstalled(
    installed: readonly KarinPluginInstalled[],
    hints: readonly PluginTaskHint[],
): KarinPluginInstalled[] {
    return overlayInstalledFromTasks(
        installed,
        hints,
        (item, name) => item.name === name,
        (name) => ({ name, kind: 'npm', enabled: true }),
    );
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
            timeLabel: catalogTimeLabel(entry.time, state.installed, state.version),
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
            timeLabel: installedLabel(item.version),
        };
        if (matchesQuery(row, query)) rows.push(row);
    }
    return rows;
}
