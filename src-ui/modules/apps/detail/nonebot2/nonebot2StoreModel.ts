// 市场 ∪ 已装 ∪ 任务 overlay。目录默认展示，分页在 Tab 里切。

import { SEE_LOGS_HINT } from '../../../../core/domain/ui/errorBarCopy';
import type {
    AppPluginAction,
    AppStoreInstalled,
    AppStoreMarketEntry,
    AppStoreResource,
    DeploymentTaskStatus,
} from '../../../../core/ipc/types';

export type StoreKindFilter = 'all' | 'official' | 'installed';

export type VisibleStoreItem = {
    id: string;
    name: string;
    description: string;
    authorName: string;
    installed: boolean;
    enabled: boolean;
    locked: boolean;
    official: boolean;
    version?: string;
    timeLabel: string | null;
    package: string;
    homepage: string;
};

const ONEBOT_V11 = 'nonebot.adapters.onebot.v11';

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

export function pluginCatalogErrorCopy(raw: string): { title: string; content: string } {
    const stripped = raw
        .replace(/^写入应用端配置失败:\s*/g, '')
        .replace(/^拉取(?:适配器|插件)?目录失败:\s*/g, '')
        .replace(/^读取(?:适配器|插件)?目录失败:\s*/g, '')
        .trim();
    if (/error sending request|timed out|connection refused|dns|network|proxy/i.test(raw)) {
        return { title: '无法连接官方目录', content: `检查网络或代理后重试。${SEE_LOGS_HINT}` };
    }
    if (/HTTP\s+[45]\d\d/.test(raw)) {
        return { title: '官方目录暂时不可用', content: `稍后重试。${SEE_LOGS_HINT}` };
    }
    if (!stripped) return { title: '目录加载失败', content: SEE_LOGS_HINT };
    return { title: '目录加载失败', content: SEE_LOGS_HINT };
}

export function parseStoreTime(time: string): number | null {
    const t = time.trim();
    if (!t) return null;
    const iso = t.includes('T') ? t : t.replace(' ', 'T');
    const ts = Date.parse(iso);
    return Number.isNaN(ts) ? null : ts;
}

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
    return null;
}

function matchesQuery(
    row: Pick<VisibleStoreItem, 'id' | 'name' | 'description' | 'authorName'>,
    query: string,
): boolean {
    const q = query.trim().toLowerCase();
    if (!q) return true;
    return (
        row.id.toLowerCase().includes(q) ||
        row.name.toLowerCase().includes(q) ||
        row.description.toLowerCase().includes(q) ||
        row.authorName.toLowerCase().includes(q)
    );
}

export type StoreTaskHint = {
    pluginName: string;
    action: AppPluginAction;
    status: DeploymentTaskStatus;
    atMs: number;
};

export function overlayInstalledFromTasks(
    installed: readonly AppStoreInstalled[],
    hints: readonly StoreTaskHint[],
    resource: AppStoreResource,
): AppStoreInstalled[] {
    const latest = new Map<string, StoreTaskHint>();
    for (const hint of hints) {
        const prev = latest.get(hint.pluginName);
        if (!prev || hint.atMs >= prev.atMs) latest.set(hint.pluginName, hint);
    }
    let next = [...installed];
    for (const hint of latest.values()) {
        if (hint.status !== 'success') continue;
        if (hint.action === 'uninstall') {
            next = next.filter((item) => item.id !== hint.pluginName && item.name !== hint.pluginName);
            continue;
        }
        if (!next.some((item) => item.id === hint.pluginName || item.name === hint.pluginName)) {
            next.push({
                id: hint.pluginName,
                name: hint.pluginName,
                resource,
                flavor: 'pypi',
                enabled: true,
                package: '',
            });
        }
    }
    return next;
}

function timeLabelFor(entry: AppStoreMarketEntry, installed: boolean, version?: string): string | null {
    if (installed) return version ? `v${version}` : '已装';
    const ts = parseStoreTime(entry.time);
    if (ts == null) return null;
    return formatRelativeTime(new Date(ts).toISOString());
}

export function filterNoneBot2Store(args: {
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

    if (kindFilter !== 'installed') {
        for (const entry of entries) {
            if (kindFilter === 'official' && !entry.is_official) continue;
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
                locked: linked && entry.id === ONEBOT_V11,
                official: entry.is_official,
                version: hit?.version,
                timeLabel: timeLabelFor(entry, !!hit, hit?.version),
                package: entry.package,
                homepage: entry.homepage,
            };
            if (!matchesQuery(row, q)) continue;
            rows.push(row);
            covered.add(entry.id);
            covered.add(entry.name);
        }
    }

    for (const item of installed) {
        if (covered.has(item.id) || covered.has(item.name)) continue;
        if (kindFilter === 'official') continue;
        const row: VisibleStoreItem = {
            id: item.id,
            name: item.name || item.id,
            description: '',
            authorName: '',
            installed: true,
            enabled: item.enabled,
            locked: linked && item.id === ONEBOT_V11,
            official: false,
            version: item.version,
            timeLabel: item.version ? `v${item.version}` : '已装',
            package: item.package,
            homepage: '',
        };
        if (!matchesQuery(row, q)) continue;
        rows.push(row);
    }
    return rows;
}

/** 量不到格子时的回退：3×4，对齐官方店默认 12 条。 */
export const STORE_PAGE_SIZE = 12;
export const STORE_GRID_GAP_PX = 12;
export const STORE_CARD_MIN_WIDTH_PX = 280;
export const STORE_CARD_MIN_HEIGHT_PX = 148;

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
