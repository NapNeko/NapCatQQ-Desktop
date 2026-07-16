// Bot 实例运行时指标：间隔 / 保留天数 clamp（对齐 ncd-domain）+ 展示 helpers

import type { BotRuntimeMetrics } from '../../ipc/generated/domain/BotRuntimeMetrics';
import type { NetworkNodeKind } from '../../ipc/generated/domain/NetworkNodeKind';
import type { NetworkNodeMetrics } from '../../ipc/generated/domain/NetworkNodeMetrics';
import type { ProbeHealth } from '../../ipc/generated/domain/ProbeHealth';

export const BOT_RUNTIME_METRICS_INTERVAL_MS_MIN = 1000;
export const BOT_RUNTIME_METRICS_INTERVAL_MS_MAX = 30_000;
export const BOT_RUNTIME_METRICS_INTERVAL_MS_DEFAULT = 3000;
export const BOT_RUNTIME_METRICS_RETENTION_DAYS_DEFAULT = 7;
export const BOT_RUNTIME_METRICS_RETENTION_DAYS_MIN = 1;
export const BOT_RUNTIME_METRICS_RETENTION_DAYS_MAX = 90;

export function clampBotRuntimeMetricsIntervalMs(raw: number): number {
    if (!Number.isFinite(raw)) {
        return BOT_RUNTIME_METRICS_INTERVAL_MS_DEFAULT;
    }
    return Math.max(
        BOT_RUNTIME_METRICS_INTERVAL_MS_MIN,
        Math.min(BOT_RUNTIME_METRICS_INTERVAL_MS_MAX, Math.round(raw)),
    );
}

export function clampBotRuntimeMetricsRetentionDays(raw: number): number {
    if (!Number.isFinite(raw)) {
        return BOT_RUNTIME_METRICS_RETENTION_DAYS_DEFAULT;
    }
    return Math.max(
        BOT_RUNTIME_METRICS_RETENTION_DAYS_MIN,
        Math.min(BOT_RUNTIME_METRICS_RETENTION_DAYS_MAX, Math.round(raw)),
    );
}

export function formatBytes(n: number | null | undefined): string {
    if (n == null || !Number.isFinite(n) || n < 0) return '—';
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
    if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
    return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

/** 卡片/KPI 用紧凑整数：1234 → 1.2k */
export function formatCompactCount(n: number | null | undefined): string {
    if (n == null || !Number.isFinite(n)) return '—';
    const v = Math.max(0, Math.floor(n));
    if (v < 1000) return String(v);
    if (v < 1_000_000) {
        const k = v / 1000;
        return `${k >= 100 ? k.toFixed(0) : k.toFixed(1).replace(/\.0$/, '')}k`;
    }
    const m = v / 1_000_000;
    return `${m >= 100 ? m.toFixed(0) : m.toFixed(1).replace(/\.0$/, '')}M`;
}

export function networkNodeKindLabel(kind: NetworkNodeKind | string | undefined): string {
    switch (kind) {
        case 'httpServer':
            return 'HTTP 服务';
        case 'httpClient':
            return 'HTTP 客户端';
        case 'httpSse':
            return 'HTTP SSE';
        case 'wsServer':
            return 'WS 服务';
        case 'wsClient':
            return 'WS 客户端';
        default:
            return '未知';
    }
}

export function probeHealthLabel(probe: ProbeHealth | string | undefined): string {
    switch (probe) {
        case 'active':
            return '正常';
        case 'stale':
            return '数据陈旧';
        case 'not_injected':
            return '未注入';
        case 'error':
            return '异常';
        default:
            return '未知';
    }
}

export interface MetricsNodeTotals {
    eventsOut: number;
    actionsIn: number;
    bytesOut: number;
    bytesIn: number;
    errors: number;
}

export function sumNodeTotals(nodes: NetworkNodeMetrics[] | undefined | null): MetricsNodeTotals {
    let eventsOut = 0;
    let actionsIn = 0;
    let bytesOut = 0;
    let bytesIn = 0;
    let errors = 0;
    for (const n of nodes ?? []) {
        eventsOut += Number(n.events_out ?? 0) || 0;
        actionsIn += Number(n.actions_in ?? 0) || 0;
        bytesOut += Number(n.bytes_out ?? 0) || 0;
        bytesIn += Number(n.bytes_in ?? 0) || 0;
        errors += Number(n.errors ?? 0) || 0;
    }
    return { eventsOut, actionsIn, bytesOut, bytesIn, errors };
}

export function rssBytesOf(metrics: BotRuntimeMetrics | null | undefined): number | null {
    const v = metrics?.memory?.rss_bytes;
    if (v == null) return null;
    const n = Number(v);
    return Number.isFinite(n) ? n : null;
}

/** 相对时间：刚采集 / N 秒前 / N 分钟前 */
export function formatCollectedAgo(collectedAtMs: number | null | undefined, nowMs = Date.now()): string {
    if (collectedAtMs == null || !Number.isFinite(collectedAtMs) || collectedAtMs <= 0) {
        return '尚无采集';
    }
    const delta = Math.max(0, nowMs - collectedAtMs);
    if (delta < 2000) return '刚刚';
    if (delta < 60_000) return `${Math.floor(delta / 1000)} 秒前`;
    if (delta < 3600_000) return `${Math.floor(delta / 60_000)} 分钟前`;
    if (delta < 86400_000) return `${Math.floor(delta / 3600_000)} 小时前`;
    return `${Math.floor(delta / 86400_000)} 天前`;
}

/** 历史图预设时间范围（受保留天数 clamp） */
export type MetricsHistoryRange = '1h' | '6h' | '24h' | '7d' | '14d' | '30d';

export const METRICS_HISTORY_RANGE_OPTIONS: {
    id: MetricsHistoryRange;
    label: string;
    /** 面板短标签 */
    shortLabel: string;
    ms: number;
}[] = [
        { id: '1h', label: '1 小时', shortLabel: '1h', ms: 3600_000 },
        { id: '6h', label: '6 小时', shortLabel: '6h', ms: 6 * 3600_000 },
        { id: '24h', label: '24 小时', shortLabel: '1d', ms: 86400_000 },
        { id: '7d', label: '7 天', shortLabel: '7d', ms: 7 * 86400_000 },
        { id: '14d', label: '14 天', shortLabel: '14d', ms: 14 * 86400_000 },
        { id: '30d', label: '30 天', shortLabel: '30d', ms: 30 * 86400_000 },
    ];

/** 趋势图查询窗口：预设 或 自定义起止 */
export type MetricsHistoryWindow =
    | { mode: 'preset'; range: MetricsHistoryRange }
    | {
        mode: 'custom';
        fromMs: number;
        toMs: number;
        /** true 时每次刷新 to = now */
        followNow: boolean;
    };

export function historyRangeToFromMs(
    range: MetricsHistoryRange,
    retentionDays: number,
    nowMs = Date.now(),
): number {
    const opt = METRICS_HISTORY_RANGE_OPTIONS.find((o) => o.id === range);
    const want = opt?.ms ?? 3600_000;
    const capDays = clampBotRuntimeMetricsRetentionDays(retentionDays);
    const capMs = capDays * 86400_000;
    return nowMs - Math.min(want, capMs);
}

export function isMetricsHistoryRangeAvailable(
    range: MetricsHistoryRange,
    retentionDays: number,
): boolean {
    const opt = METRICS_HISTORY_RANGE_OPTIONS.find((o) => o.id === range);
    if (!opt) return false;
    const capMs = clampBotRuntimeMetricsRetentionDays(retentionDays) * 86400_000;
    // 允许选不超过保留窗口的范围；1h 始终可用
    return opt.ms <= capMs || range === '1h';
}

export function resolveHistoryWindowBounds(
    window: MetricsHistoryWindow,
    retentionDays: number,
    nowMs = Date.now(),
): { fromMs: number; toMs: number } {
    const capMs = clampBotRuntimeMetricsRetentionDays(retentionDays) * 86400_000;
    const earliest = nowMs - capMs;

    if (window.mode === 'preset') {
        return {
            fromMs: historyRangeToFromMs(window.range, retentionDays, nowMs),
            toMs: nowMs,
        };
    }

    let toMs = window.followNow ? nowMs : window.toMs;
    let fromMs = window.fromMs;
    if (!Number.isFinite(toMs) || toMs <= 0) toMs = nowMs;
    if (!Number.isFinite(fromMs) || fromMs <= 0) fromMs = toMs - 3600_000;
    if (fromMs > toMs) {
        const t = fromMs;
        fromMs = toMs;
        toMs = t;
    }
    // 不超出保留窗口
    fromMs = Math.max(fromMs, earliest);
    toMs = Math.min(Math.max(toMs, fromMs + 60_000), nowMs);
    return { fromMs, toMs };
}

export function formatHistoryWindowLabel(window: MetricsHistoryWindow): string {
    if (window.mode === 'preset') {
        return (
            METRICS_HISTORY_RANGE_OPTIONS.find((o) => o.id === window.range)?.label ??
            '时间范围'
        );
    }
    const fmt = (ms: number) =>
        new Intl.DateTimeFormat('zh-CN', {
            month: 'numeric',
            day: 'numeric',
            hour: '2-digit',
            minute: '2-digit',
        }).format(new Date(ms));
    if (window.followNow) {
        return `${fmt(window.fromMs)} → 现在`;
    }
    return `${fmt(window.fromMs)} → ${fmt(window.toMs)}`;
}

/** datetime-local 用本地墙钟字符串 */
export function msToDatetimeLocalValue(ms: number): string {
    if (!Number.isFinite(ms)) return '';
    const d = new Date(ms);
    const pad = (n: number) => String(n).padStart(2, '0');
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

export function datetimeLocalValueToMs(value: string): number | null {
    if (!value) return null;
    const t = new Date(value).getTime();
    return Number.isFinite(t) ? t : null;
}

/** 本地日 00:00 */
export function startOfLocalDay(ms: number): number {
    const d = new Date(ms);
    d.setHours(0, 0, 0, 0);
    return d.getTime();
}

/** 把某日的年月日接到另一时刻的时分秒上 */
export function combineLocalDateAndTime(dateMs: number, timeSourceMs: number): number {
    const day = new Date(dateMs);
    const time = new Date(timeSourceMs);
    day.setHours(time.getHours(), time.getMinutes(), time.getSeconds(), 0);
    return day.getTime();
}

export function formatLocalDateLabel(ms: number): string {
    if (!Number.isFinite(ms)) return '—';
    return new Intl.DateTimeFormat('zh-CN', {
        year: 'numeric',
        month: '2-digit',
        day: '2-digit',
    }).format(new Date(ms));
}

export function formatLocalTimeLabel(ms: number): string {
    if (!Number.isFinite(ms)) return '—';
    return new Intl.DateTimeFormat('zh-CN', {
        hour: '2-digit',
        minute: '2-digit',
        hour12: false,
    }).format(new Date(ms));
}
