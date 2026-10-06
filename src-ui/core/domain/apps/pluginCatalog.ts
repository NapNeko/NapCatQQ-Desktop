// Karin 插件市场和应用端商店（NoneBot2 / AstrBot / MaiBot）共用的规则：目录时间、搜索、拉目录失败的文案，
// 以及装 / 更新 / 卸载任务。这些任务都是部署队列里的 app_plugin，已装扫描跟不上时拿最近一条成功的任务补显示。

import { SEE_LOGS_HINT } from '../ui/errorBarCopy';
import { relativeTimeFromMs } from '../ui/relativeTime';
import type {
    AppPluginAction,
    AppStoreResource,
    DeploymentTaskKind,
    DeploymentTaskSnapshot,
    DeploymentTaskStatus,
} from '../../ipc/types';

export type AppPluginTask = DeploymentTaskSnapshot & {
    kind: Extract<DeploymentTaskKind, { kind: 'app_plugin' }>;
};

/** 这个实例这类资源的任务；老任务没带 resource 的算插件 */
export function isPluginTaskOf(
    task: DeploymentTaskSnapshot,
    instanceId: string,
    resource: AppStoreResource,
): task is AppPluginTask {
    return (
        task.kind.kind === 'app_plugin' &&
        task.kind.instance_id === instanceId &&
        (task.kind.resource ?? 'plugin') === resource
    );
}

export function isPluginTaskActive(status: DeploymentTaskStatus): boolean {
    return status === 'queued' || status === 'running' || status === 'waiting_input';
}

export function isPluginTaskDone(status: DeploymentTaskStatus): boolean {
    return status === 'success' || status === 'failed' || status === 'cancelled';
}

export function pluginActionVerb(action: AppPluginAction): string {
    if (action === 'install') return '安装';
    if (action === 'update') return '更新';
    return '卸载';
}

export type PluginTaskHint = {
    pluginName: string;
    action: AppPluginAction;
    status: DeploymentTaskStatus;
    atMs: number;
};

function toMs(value: bigint | number | null | undefined): number {
    if (value == null) return 0;
    return typeof value === 'bigint' ? Number(value) : value;
}

/** 任务最后一次有动静的时间：先看结束，没结束看开始，还没开始看提交 */
export function pluginTaskHint(task: AppPluginTask): PluginTaskHint {
    return {
        pluginName: task.kind.plugin_name,
        action: task.kind.action,
        status: task.status,
        atMs: toMs(task.endedAtMs) || toMs(task.startedAtMs) || toMs(task.submittedAtMs),
    };
}

/** 扫描还没跟上时，按每个名字最近一条成功的任务补「已装 / 已卸」；placeholder 造一条刚装上的占位 */
export function overlayInstalledFromTasks<T>(
    installed: readonly T[],
    hints: readonly PluginTaskHint[],
    matches: (item: T, name: string) => boolean,
    placeholder: (name: string) => T,
): T[] {
    const latest = new Map<string, PluginTaskHint>();
    for (const hint of hints) {
        const prev = latest.get(hint.pluginName);
        if (!prev || hint.atMs >= prev.atMs) latest.set(hint.pluginName, hint);
    }
    let next = [...installed];
    for (const hint of latest.values()) {
        if (hint.status !== 'success') continue;
        if (hint.action === 'uninstall') {
            next = next.filter((item) => !matches(item, hint.pluginName));
            continue;
        }
        if (!next.some((item) => matches(item, hint.pluginName))) {
            next.push(placeholder(hint.pluginName));
        }
    }
    return next;
}

/** 官方目录的时间多是「2025-01-19 10:00:00」，空格换成 T 再解析 */
export function parseCatalogTime(time: string): number | null {
    const t = time.trim();
    if (!t) return null;
    const iso = t.includes('T') ? t : t.replace(' ', 'T');
    const ts = Date.parse(iso);
    return Number.isNaN(ts) ? null : ts;
}

export function installedLabel(version?: string): string {
    return version ? `v${version}` : '已装';
}

/** 卡片副标题的时间位：装了的写版本，没装的写上架多久；给了 maxDays 的，更早的不写 */
export function catalogTimeLabel(
    time: string,
    installed: boolean,
    version?: string,
    maxDays?: number,
): string | null {
    if (installed) return installedLabel(version);
    const ts = parseCatalogTime(time);
    if (ts == null) return null;
    return relativeTimeFromMs(ts, maxDays);
}

/** 任一字段包含搜索词（不分大小写）就算命中；空搜索全要 */
export function matchesCatalogQuery(query: string, fields: readonly string[]): boolean {
    const q = query.trim().toLowerCase();
    if (!q) return true;
    return fields.some((field) => field.toLowerCase().includes(q));
}

/**
 * 目录拉不下来时的短文案。连不上和对方出错分开说，其余一律指去日志：原文进日志，不在条上堆。
 * catalog 是标题里的叫法，Karin 叫「插件目录」，商店插件、适配器共用一个「目录」。
 */
export function pluginCatalogErrorCopy(
    raw: string,
    catalog = '目录',
): { title: string; content: string } {
    if (/error sending request|timed out|connection refused|dns|network|proxy/i.test(raw)) {
        return {
            title: `无法连接官方${catalog}`,
            content: `检查网络或代理后重试。${SEE_LOGS_HINT}`,
        };
    }
    if (/HTTP\s+[45]\d\d/.test(raw)) {
        return { title: `官方${catalog}暂时不可用`, content: `稍后重试。${SEE_LOGS_HINT}` };
    }
    return { title: `${catalog}加载失败`, content: SEE_LOGS_HINT };
}
