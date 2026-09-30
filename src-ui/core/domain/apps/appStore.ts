// 应用端商店（NoneBot2 的适配器 / 插件，AstrBot、MaiBot 的插件）：市场 ∪ 已装 ∪ 任务 overlay。
// 目录默认展示，分页在 Tab 里切。时间、搜索、任务补显示和 Karin 插件市场一套，在 pluginCatalog。

import {
    catalogTimeLabel,
    installedLabel,
    matchesCatalogQuery,
    overlayInstalledFromTasks,
    type PluginTaskHint,
} from './pluginCatalog';
import type { AppStoreInstalled, AppStoreMarketEntry, AppStoreResource } from '../../ipc/types';

/** `tag:<分类>` 按目录给的分类筛（云崽的索引按功能 / 游戏 / 文游 / 单 JS 分表） */
export type StoreKindFilter = 'all' | 'official' | 'installed' | `tag:${string}`;

export type VisibleStoreItem = {
    id: string;
    name: string;
    description: string;
    authorName: string;
    installed: boolean;
    enabled: boolean;
    locked: boolean;
    /** 为什么不能动：对接着的 OneBot 适配器、桌面端管着的 MaiBot NapCat 适配器 */
    lockReason?: string;
    /** 桌面端自己装、自己写配置的：连更新也不给，版本跟着应用端本体走 */
    managed?: boolean;
    official: boolean;
    version?: string;
    timeLabel: string | null;
    package: string;
    homepage: string;
    tags: readonly string[];
    /** 目录条目给了能装的东西（仓库 / 文件）；给不出来的只能去主页手动装 */
    installable: boolean;
};

/** 后端解析目录时认不出能装的东西就标 valid=false（云崽索引里源码链接不是 .js、主页不是仓库的那些）；
 *  别的框架的条目都是 true。AstrBot / 麦麦的 git 条目按主页装，repos 本来就是空的，不能拿它判 */
export function storeEntryInstallable(entry: AppStoreMarketEntry): boolean {
    return entry.valid;
}

const ONEBOT_V11 = 'nonebot.adapters.onebot.v11';

/** 后端标了 locked 的是桌面端自己管的；NoneBot 的 OneBot 适配器是对接着才不让关 */
function lockOf(
    id: string,
    installed: AppStoreInstalled | undefined,
    linked: boolean,
): Pick<VisibleStoreItem, 'locked' | 'lockReason' | 'managed'> {
    if (installed?.locked) return { locked: true, lockReason: '桌面端管理', managed: true };
    if (linked && id === ONEBOT_V11) return { locked: true, lockReason: '已对接不能关' };
    return { locked: false };
}

export function storeOpErrorCopy(raw: string): string {
    const stripped = raw
        .replace(/^应用端运行失败:\s*/g, '')
        .replace(/^uv add:\s*exit=[^:]+:\s*/g, '')
        .trim();
    const built = /Failed to build [`']([A-Za-z0-9_.-]+)==([^`']+)/.exec(raw);
    if (built) {
        return `依赖 ${built[1]} ${built[2]} 无法编译，当前环境没有可用的预编译包`;
    }
    if (/--frozen|Build failures usually indicate/i.test(raw)) {
        return '依赖无法安装，当前环境没有可用的预编译包，或这个插件过旧';
    }
    if (/error sending request|timed out|connection refused|dns|network|proxy/i.test(raw)) {
        return '无法连接软件源，检查网络或代理后重试';
    }
    if (!stripped) return '安装失败';
    return stripped.length > 160 ? `${stripped.slice(0, 160)}…` : stripped;
}

function matchesQuery(row: VisibleStoreItem, query: string): boolean {
    return matchesCatalogQuery(query, [row.id, row.name, row.description, row.authorName]);
}

/** 任务里的名字是 id，也可能是显示名，两个都认 */
export function overlayStoreInstalled(
    installed: readonly AppStoreInstalled[],
    hints: readonly PluginTaskHint[],
    resource: AppStoreResource,
): AppStoreInstalled[] {
    return overlayInstalledFromTasks(
        installed,
        hints,
        (item, name) => item.id === name || item.name === name,
        (name) => ({
            id: name,
            name,
            resource,
            flavor: 'pypi',
            enabled: true,
            package: '',
            locked: false,
        }),
    );
}

// 商店里大多是上架很久的，一周以前的不写，免得满屏「几年前」
const TIME_LABEL_MAX_DAYS = 7;

/** enabledAdapterModules 只有 NoneBot2 有：插件声明了支持哪些适配器的，跟启用着的对不上就不列 */
export function filterAppStore(args: {
    resource: AppStoreResource;
    entries: readonly AppStoreMarketEntry[];
    installed: readonly AppStoreInstalled[];
    query: string;
    kindFilter: StoreKindFilter;
    enabledAdapterModules: readonly string[];
    linked: boolean;
}): VisibleStoreItem[] {
    const { resource, entries, installed, query, kindFilter, enabledAdapterModules, linked } = args;
    const q = query.trim();
    const rows: VisibleStoreItem[] = [];
    const covered = new Set<string>();

    const tag = kindFilter.startsWith('tag:') ? kindFilter.slice(4) : null;
    if (kindFilter !== 'installed') {
        for (const entry of entries) {
            if (kindFilter === 'official' && !entry.is_official) continue;
            if (tag !== null && !entry.tags.includes(tag)) continue;
            if (
                resource === 'plugin'
                && entry.supported_adapters.length > 0
                && enabledAdapterModules.length > 0
                && !entry.supported_adapters.some((m) => enabledAdapterModules.includes(m))
            ) {
                continue;
            }
            const hit = installed.find((i) => i.id === entry.id || i.name === entry.name);
            const row: VisibleStoreItem = {
                id: entry.id,
                name: entry.name || entry.id,
                description: entry.description,
                authorName: entry.author,
                installed: !!hit,
                enabled: hit?.enabled ?? false,
                ...lockOf(entry.id, hit, linked),
                official: entry.is_official,
                version: hit?.version,
                timeLabel: catalogTimeLabel(entry.time, !!hit, hit?.version, TIME_LABEL_MAX_DAYS),
                package: entry.package,
                homepage: entry.homepage,
                tags: entry.tags,
                installable: storeEntryInstallable(entry),
            };
            if (!matchesQuery(row, q)) continue;
            rows.push(row);
            covered.add(entry.id);
            covered.add(entry.name);
        }
    }

    for (const item of installed) {
        if (covered.has(item.id) || covered.has(item.name)) continue;
        if (kindFilter === 'official' || tag !== null) continue;
        const row: VisibleStoreItem = {
            id: item.id,
            name: item.name || item.id,
            description: '',
            authorName: '',
            installed: true,
            enabled: item.enabled,
            ...lockOf(item.id, item, linked),
            official: false,
            version: item.version,
            timeLabel: installedLabel(item.version),
            package: item.package,
            homepage: '',
            tags: [],
            installable: true,
        };
        if (!matchesQuery(row, q)) continue;
        rows.push(row);
    }
    return rows;
}

/** 量不到格子时的回退：3×4，对齐官方店默认 12 条。 */
const STORE_PAGE_SIZE = 12;
const STORE_GRID_GAP_PX = 12;
const STORE_CARD_MIN_WIDTH_PX = 280;
const STORE_CARD_MIN_HEIGHT_PX = 148;

export type StoreGridFit = {
    cols: number;
    rows: number;
    pageSize: number;
};

/** 按可视区域算出刚好铺满、不用滚的行列。 */
export function storeGridFit(width: number, height: number): StoreGridFit {
    if (!Number.isFinite(width) || !Number.isFinite(height) || width < 1 || height < 1) {
        return { cols: 3, rows: 4, pageSize: STORE_PAGE_SIZE };
    }
    const cols = Math.min(
        4,
        Math.max(1, Math.floor((width + STORE_GRID_GAP_PX) / (STORE_CARD_MIN_WIDTH_PX + STORE_GRID_GAP_PX))),
    );
    const rows = Math.min(
        4,
        Math.max(1, Math.floor((height + STORE_GRID_GAP_PX) / (STORE_CARD_MIN_HEIGHT_PX + STORE_GRID_GAP_PX))),
    );
    return { cols, rows, pageSize: cols * rows };
}

export function paginateStore<T>(rows: readonly T[], page: number, pageSize = STORE_PAGE_SIZE): T[] {
    if (rows.length === 0) return [];
    const totalPages = Math.max(1, Math.ceil(rows.length / pageSize));
    const safe = Math.min(Math.max(page, 0), totalPages - 1);
    const start = safe * pageSize;
    return rows.slice(start, start + pageSize);
}

export function storePageItems(page: number, total: number): Array<number | 'gap'> {
    if (total <= 1) return [0];
    if (total <= 11) return Array.from({ length: total }, (_, i) => i);

    let start = Math.max(1, page - 3);
    let end = Math.min(total - 2, page + 3);
    while (end - start + 1 < 7 && start > 1) start -= 1;
    while (end - start + 1 < 7 && end < total - 2) end += 1;

    const out: Array<number | 'gap'> = [0];
    if (start > 1) out.push('gap');
    for (let i = start; i <= end; i++) out.push(i);
    if (end < total - 2) out.push('gap');
    out.push(total - 1);
    return out;
}

export { ONEBOT_V11 };
