import { describe, expect, it } from 'vitest';
import type { BotRuntimeMetrics } from '../../ipc/generated/domain/BotRuntimeMetrics';
import type { NetworkNodeMetrics } from '../../ipc/generated/domain/NetworkNodeMetrics';
import {
    BOT_RUNTIME_METRICS_INTERVAL_MS_DEFAULT,
    BOT_RUNTIME_METRICS_RETENTION_DAYS_DEFAULT,
    METRICS_HISTORY_RANGE_OPTIONS,
    clampBotRuntimeMetricsIntervalMs,
    clampBotRuntimeMetricsRetentionDays,
    combineLocalDateAndTime,
    datetimeLocalValueToMs,
    formatBytes,
    formatCollectedAgo,
    formatCompactCount,
    formatHistoryWindowLabel,
    formatLocalDateLabel,
    formatLocalTimeLabel,
    historyRangeToFromMs,
    isMetricsHistoryRangeAvailable,
    msToDatetimeLocalValue,
    networkNodeKindLabel,
    probeHealthLabel,
    resolveHistoryWindowBounds,
    rssBytesOf,
    startOfLocalDay,
    sumNodeTotals,
} from './runtime-metrics-settings';

const HOUR = 3600_000;
const DAY = 86400_000;

describe('clamp 采集间隔与保留天数', () => {
    it('非法数字一律回默认值', () => {
        for (const raw of [Number.NaN, Number.POSITIVE_INFINITY, Number.NEGATIVE_INFINITY]) {
            expect(clampBotRuntimeMetricsIntervalMs(raw)).toBe(
                BOT_RUNTIME_METRICS_INTERVAL_MS_DEFAULT,
            );
            expect(clampBotRuntimeMetricsRetentionDays(raw)).toBe(
                BOT_RUNTIME_METRICS_RETENTION_DAYS_DEFAULT,
            );
        }
    });

    it('间隔夹在 1000-30000 并取整', () => {
        expect(clampBotRuntimeMetricsIntervalMs(999)).toBe(1000);
        expect(clampBotRuntimeMetricsIntervalMs(30_001)).toBe(30_000);
        expect(clampBotRuntimeMetricsIntervalMs(-5)).toBe(1000);
        expect(clampBotRuntimeMetricsIntervalMs(2500.4)).toBe(2500);
        expect(clampBotRuntimeMetricsIntervalMs(1000)).toBe(1000);
        expect(clampBotRuntimeMetricsIntervalMs(30_000)).toBe(30_000);
    });

    it('保留天数夹在 1-90 并取整', () => {
        expect(clampBotRuntimeMetricsRetentionDays(0)).toBe(1);
        expect(clampBotRuntimeMetricsRetentionDays(91)).toBe(90);
        expect(clampBotRuntimeMetricsRetentionDays(7.6)).toBe(8);
        expect(clampBotRuntimeMetricsRetentionDays(30)).toBe(30);
    });
});

describe('formatBytes', () => {
    it('无值 / 负数 / 非有限显示占位符', () => {
        expect(formatBytes(null)).toBe('—');
        expect(formatBytes(undefined)).toBe('—');
        expect(formatBytes(Number.NaN)).toBe('—');
        expect(formatBytes(-1)).toBe('—');
    });

    it('按 1024 进位并固定小数位', () => {
        expect(formatBytes(0)).toBe('0 B');
        expect(formatBytes(1023)).toBe('1023 B');
        expect(formatBytes(1024)).toBe('1.0 KB');
        expect(formatBytes(1536)).toBe('1.5 KB');
        expect(formatBytes(1024 * 1024)).toBe('1.0 MB');
        expect(formatBytes(200 * 1024 * 1024)).toBe('200.0 MB');
        expect(formatBytes(1024 * 1024 * 1024)).toBe('1.00 GB');
        expect(formatBytes(3.5 * 1024 * 1024 * 1024)).toBe('3.50 GB');
    });
});

describe('formatCompactCount', () => {
    it('null / NaN 显示占位符，负数按 0', () => {
        expect(formatCompactCount(null)).toBe('—');
        expect(formatCompactCount(Number.NaN)).toBe('—');
        expect(formatCompactCount(-5)).toBe('0');
    });

    it('k / M 分档且整数档不带 .0', () => {
        expect(formatCompactCount(999)).toBe('999');
        expect(formatCompactCount(1000)).toBe('1k');
        expect(formatCompactCount(1234)).toBe('1.2k');
        expect(formatCompactCount(15_000)).toBe('15k');
        expect(formatCompactCount(123_456)).toBe('123k');
        expect(formatCompactCount(1_000_000)).toBe('1M');
        expect(formatCompactCount(2_500_000)).toBe('2.5M');
        expect(formatCompactCount(123_456_789)).toBe('123M');
    });
});

describe('标签映射', () => {
    it('网络节点 kind 全量映射，未知回「未知」', () => {
        expect(networkNodeKindLabel('httpServer')).toBe('HTTP 服务');
        expect(networkNodeKindLabel('httpClient')).toBe('HTTP 客户端');
        expect(networkNodeKindLabel('httpSse')).toBe('HTTP SSE');
        expect(networkNodeKindLabel('wsServer')).toBe('WS 服务');
        expect(networkNodeKindLabel('wsClient')).toBe('WS 客户端');
        expect(networkNodeKindLabel('somethingNew')).toBe('未知');
        expect(networkNodeKindLabel(undefined)).toBe('未知');
    });

    it('探针健康度全量映射', () => {
        expect(probeHealthLabel('active')).toBe('正常');
        expect(probeHealthLabel('stale')).toBe('数据陈旧');
        expect(probeHealthLabel('not_injected')).toBe('未注入');
        expect(probeHealthLabel('error')).toBe('异常');
        expect(probeHealthLabel(undefined)).toBe('未知');
    });
});

describe('sumNodeTotals', () => {
    const node = (over: Partial<NetworkNodeMetrics> = {}): NetworkNodeMetrics => ({
        name: 'ws-server-100',
        kind: 'wsServer',
        ...over,
    });

    it('空输入给出全 0', () => {
        expect(sumNodeTotals(undefined)).toEqual({
            eventsOut: 0,
            actionsIn: 0,
            bytesOut: 0,
            bytesIn: 0,
            errors: 0,
        });
        expect(sumNodeTotals(null).eventsOut).toBe(0);
    });

    it('跨节点求和，缺字段当 0', () => {
        const totals = sumNodeTotals([
            node({ events_out: 120, actions_in: 8, bytes_out: 4096, bytes_in: 2048, errors: 1 }),
            node({ kind: 'httpClient', events_out: 30, bytes_in: 512 }),
        ]);
        expect(totals).toEqual({
            eventsOut: 150,
            actionsIn: 8,
            bytesOut: 4096,
            bytesIn: 2560,
            errors: 1,
        });
    });
});

describe('rssBytesOf', () => {
    const metrics = (memory?: BotRuntimeMetrics['memory']): BotRuntimeMetrics => ({
        v: 1,
        bot_id: '10001',
        collected_at_ms: 1,
        source: 'probe',
        memory,
        nodes: [],
        probe: 'active',
    });

    it('缺内存块 / 非有限值都回 null', () => {
        expect(rssBytesOf(null)).toBeNull();
        expect(rssBytesOf(undefined)).toBeNull();
        expect(rssBytesOf(metrics())).toBeNull();
        expect(rssBytesOf(metrics({ rss_bytes: Number.NaN }))).toBeNull();
    });

    it('正常透传 rss_bytes', () => {
        expect(rssBytesOf(metrics({ rss_bytes: 268_435_456 }))).toBe(268_435_456);
    });
});

describe('formatCollectedAgo', () => {
    const now = new Date(2026, 9, 8, 12, 0, 0).getTime();

    it('未采集与未来时间戳', () => {
        expect(formatCollectedAgo(null, now)).toBe('尚无采集');
        expect(formatCollectedAgo(0, now)).toBe('尚无采集');
        expect(formatCollectedAgo(now + 10_000, now)).toBe('刚刚');
    });

    it('秒 / 分 / 时 / 天档位边界', () => {
        expect(formatCollectedAgo(now - 1999, now)).toBe('刚刚');
        expect(formatCollectedAgo(now - 2000, now)).toBe('2 秒前');
        expect(formatCollectedAgo(now - 59_000, now)).toBe('59 秒前');
        expect(formatCollectedAgo(now - 60_000, now)).toBe('1 分钟前');
        expect(formatCollectedAgo(now - HOUR, now)).toBe('1 小时前');
        expect(formatCollectedAgo(now - DAY, now)).toBe('1 天前');
    });
});

describe('历史图时间窗', () => {
    const now = new Date(2026, 9, 8, 12, 0, 0).getTime();

    it('预设选项覆盖 1h-30d', () => {
        expect(METRICS_HISTORY_RANGE_OPTIONS.map((o) => o.id)).toEqual([
            '1h',
            '6h',
            '24h',
            '7d',
            '14d',
            '30d',
        ]);
    });

    it('fromMs 不越过保留窗口', () => {
        expect(historyRangeToFromMs('1h', 7, now)).toBe(now - HOUR);
        expect(historyRangeToFromMs('30d', 7, now)).toBe(now - 7 * DAY);
        expect(historyRangeToFromMs('30d', 90, now)).toBe(now - 30 * DAY);
    });

    it('超出保留窗口的预设不可选，1h 始终可选', () => {
        expect(isMetricsHistoryRangeAvailable('7d', 7)).toBe(true);
        expect(isMetricsHistoryRangeAvailable('14d', 7)).toBe(false);
        expect(isMetricsHistoryRangeAvailable('1h', 1)).toBe(true);
        expect(isMetricsHistoryRangeAvailable('6h', 1)).toBe(true);
    });

    it('预设窗口的 to 就是当前时刻', () => {
        const bounds = resolveHistoryWindowBounds({ mode: 'preset', range: '6h' }, 7, now);
        expect(bounds).toEqual({ fromMs: now - 6 * HOUR, toMs: now });
    });

    it('自定义窗口：followNow 跟随当前、起止倒置交换、裁剪进保留窗口', () => {
        const follow = resolveHistoryWindowBounds(
            { mode: 'custom', fromMs: now - 2 * HOUR, toMs: 0, followNow: true },
            7,
            now,
        );
        expect(follow).toEqual({ fromMs: now - 2 * HOUR, toMs: now });

        const swapped = resolveHistoryWindowBounds(
            { mode: 'custom', fromMs: now, toMs: now - HOUR, followNow: false },
            7,
            now,
        );
        expect(swapped).toEqual({ fromMs: now - HOUR, toMs: now });

        const clipped = resolveHistoryWindowBounds(
            { mode: 'custom', fromMs: now - 30 * DAY, toMs: now - DAY, followNow: false },
            7,
            now,
        );
        expect(clipped).toEqual({ fromMs: now - 7 * DAY, toMs: now - DAY });
    });

    it('自定义窗口至少留 60 秒跨度且不越过 now', () => {
        const tight = resolveHistoryWindowBounds(
            { mode: 'custom', fromMs: now - 61_000, toMs: now - 60_500, followNow: false },
            30,
            now,
        );
        expect(tight.toMs - tight.fromMs).toBe(60_000);

        const future = resolveHistoryWindowBounds(
            { mode: 'custom', fromMs: now - HOUR, toMs: now + DAY, followNow: false },
            30,
            now,
        );
        expect(future.toMs).toBe(now);
    });

    it('标签：预设用中文名，自定义带「现在」或两段时刻', () => {
        expect(formatHistoryWindowLabel({ mode: 'preset', range: '24h' })).toBe('24 小时');
        const follow = formatHistoryWindowLabel({
            mode: 'custom',
            fromMs: now - HOUR,
            toMs: now,
            followNow: true,
        });
        expect(follow).toContain('现在');
        const fixed = formatHistoryWindowLabel({
            mode: 'custom',
            fromMs: now - 2 * HOUR,
            toMs: now - HOUR,
            followNow: false,
        });
        expect(fixed).not.toContain('现在');
        expect(fixed.split('→')).toHaveLength(2);
    });
});

describe('本地墙钟换算', () => {
    it('datetime-local 字符串与毫秒往返（分钟精度）', () => {
        const ms = new Date(2026, 9, 8, 9, 5, 0, 0).getTime();
        expect(msToDatetimeLocalValue(ms)).toBe('2026-10-08T09:05');
        expect(datetimeLocalValueToMs('2026-10-08T09:05')).toBe(ms);
        expect(msToDatetimeLocalValue(Number.NaN)).toBe('');
        expect(datetimeLocalValueToMs('')).toBeNull();
        expect(datetimeLocalValueToMs('不是时间')).toBeNull();
    });

    it('startOfLocalDay 归到本地 00:00', () => {
        const noon = new Date(2026, 9, 8, 13, 45, 30).getTime();
        expect(startOfLocalDay(noon)).toBe(new Date(2026, 9, 8, 0, 0, 0, 0).getTime());
    });

    it('combineLocalDateAndTime 保留日期换时刻', () => {
        const dateMs = new Date(2026, 9, 1, 0, 0).getTime();
        const timeMs = new Date(2026, 9, 8, 13, 45, 30).getTime();
        expect(combineLocalDateAndTime(dateMs, timeMs)).toBe(
            new Date(2026, 9, 1, 13, 45, 30, 0).getTime(),
        );
    });

    it('日期 / 时刻标签：非法输入占位，正常输入含对应数字', () => {
        const ms = new Date(2026, 9, 8, 13, 5, 0).getTime();
        expect(formatLocalDateLabel(Number.NaN)).toBe('—');
        expect(formatLocalTimeLabel(Number.NaN)).toBe('—');
        expect(formatLocalDateLabel(ms)).toMatch(/2026\D+10\D+08/);
        expect(formatLocalTimeLabel(ms)).toContain('13:05');
    });
});
