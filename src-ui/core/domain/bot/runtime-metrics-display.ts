// 指标页展示派生：主机内存 / CPU / 磁盘占比、进程 RSS 占比、活跃节点数、探针徽章色、
// 节点活动时间文案、趋势序列标题。原先散在指标页组件体内做内联算术，挪出来让
// UI 层只做编排，同时给这些边界判断（缺字段 / NaN / 除零）配单测。

import type { BotRuntimeMetrics } from '../../ipc/generated/domain/BotRuntimeMetrics';
import type { NetworkNodeMetrics } from '../../ipc/generated/domain/NetworkNodeMetrics';
import type { ProbeHealth } from '../../ipc/generated/domain/ProbeHealth';
import { formatCollectedAgo, rssBytesOf } from './runtime-metrics-settings';

/** 节点活动列：无效 / 零时间显示占位破折号 */
export function formatActivity(ms: number | null | undefined, nowMs = Date.now()): string {
    if (ms == null || !Number.isFinite(Number(ms)) || Number(ms) <= 0) return '—';
    return formatCollectedAgo(Number(ms), nowMs);
}

/** 主机物理内存占用比例 0-1；缺任一侧或总量非正时 null（UI 画空槽） */
export function hostMemoryRatioOf(metrics: BotRuntimeMetrics | null | undefined): number | null {
    const used = metrics?.memory?.host_used_bytes;
    const total = metrics?.memory?.host_total_bytes;
    return used != null && total != null && Number(total) > 0 && Number.isFinite(Number(used))
        ? Number(used) / Number(total)
        : null;
}

/** 主机 CPU 占用 0-100，钳到区间；缺字段 / NaN 时 null */
export function hostCpuPercentOf(metrics: BotRuntimeMetrics | null | undefined): number | null {
    const cpu = metrics?.memory?.host_cpu_percent;
    return cpu != null && Number.isFinite(Number(cpu))
        ? Math.max(0, Math.min(100, Number(cpu)))
        : null;
}

/** 主机磁盘占用比例 0-1；缺任一侧或总量非正时 null */
export function hostDiskRatioOf(metrics: BotRuntimeMetrics | null | undefined): number | null {
    const used = metrics?.memory?.host_disk_used_bytes;
    const total = metrics?.memory?.host_disk_total_bytes;
    return used != null && total != null && Number(total) > 0 && Number.isFinite(Number(used))
        ? Number(used) / Number(total)
        : null;
}

/** 进程 RSS 占主机内存总量比例 0-1；缺 RSS 或总量非正时 null */
export function rssRatioOfHostTotal(metrics: BotRuntimeMetrics | null | undefined): number | null {
    const rss = rssBytesOf(metrics);
    const total = metrics?.memory?.host_total_bytes;
    return rss != null && total != null && Number(total) > 0 ? rss / Number(total) : null;
}

/** 有过活动（last_activity_at_ms > 0）的节点数 */
export function activeNodesCountOf(nodes: NetworkNodeMetrics[] | null | undefined): number {
    return (nodes ?? []).filter((node) => Number(node.last_activity_at_ms ?? 0) > 0).length;
}

/** 探针健康 → 顶栏徽章色调 */
export function probeBadgeTone(
    probe: ProbeHealth | string | undefined,
): 'success' | 'danger' | 'warning' | 'neutral' {
    if (probe === 'active') return 'success';
    if (probe === 'error') return 'danger';
    if (probe === 'stale') return 'warning';
    return 'neutral';
}

/** 趋势序列 key → 图表标题；与 HistorySeriesKey 同形，domain 不 import UI 层类型 */
export function historySeriesTitle(series: 'rss' | 'eventsOut' | 'actionsIn'): string {
    if (series === 'rss') return '内存 RSS';
    if (series === 'eventsOut') return '出站事件';
    return '入站 action';
}
