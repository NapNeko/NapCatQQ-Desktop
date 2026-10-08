import { describe, expect, it } from 'vitest';

import type { BotRuntimeMetrics } from '../../ipc/generated/domain/BotRuntimeMetrics';
import type { NetworkNodeMetrics } from '../../ipc/generated/domain/NetworkNodeMetrics';
import {
    activeNodesCountOf,
    formatActivity,
    historySeriesTitle,
    hostCpuPercentOf,
    hostDiskRatioOf,
    hostMemoryRatioOf,
    probeBadgeTone,
    rssRatioOfHostTotal,
} from './runtime-metrics-display';

function metricsWith(memory: BotRuntimeMetrics['memory']): BotRuntimeMetrics {
    return {
        v: 1,
        bot_id: '10001',
        collected_at_ms: 1_700_000_000_000,
        source: 'probe',
        memory,
        nodes: [],
        probe: 'active',
    };
}

function node(lastActivityMs: number | undefined): NetworkNodeMetrics {
    return { name: 'n', kind: 'httpServer', last_activity_at_ms: lastActivityMs };
}

describe('formatActivity', () => {
    it('缺失、非有限数与非正时间显示占位符', () => {
        expect(formatActivity(null)).toBe('—');
        expect(formatActivity(undefined)).toBe('—');
        expect(formatActivity(0)).toBe('—');
        expect(formatActivity(-5)).toBe('—');
        expect(formatActivity(Number.NaN)).toBe('—');
    });

    it('有效时间按相对时间展示', () => {
        const now = 1_700_000_000_000;
        expect(formatActivity(now - 30_000, now)).toBe('30 秒前');
    });
});

describe('hostMemoryRatioOf', () => {
    it('已用除以总量得到比例', () => {
        const m = metricsWith({ host_used_bytes: 4, host_total_bytes: 8 });
        expect(hostMemoryRatioOf(m)).toBe(0.5);
    });

    it('缺任一字段或总量非正返回 null', () => {
        expect(hostMemoryRatioOf(metricsWith(undefined))).toBeNull();
        expect(hostMemoryRatioOf(metricsWith({ host_used_bytes: 4 }))).toBeNull();
        expect(hostMemoryRatioOf(metricsWith({ host_total_bytes: 8 }))).toBeNull();
        expect(
            hostMemoryRatioOf(metricsWith({ host_used_bytes: 4, host_total_bytes: 0 })),
        ).toBeNull();
    });

    it('已用为 NaN 时返回 null 而不是 NaN', () => {
        const m = metricsWith({ host_used_bytes: Number.NaN, host_total_bytes: 8 });
        expect(hostMemoryRatioOf(m)).toBeNull();
    });

    it('整个 metrics 缺失返回 null', () => {
        expect(hostMemoryRatioOf(null)).toBeNull();
    });
});

describe('hostCpuPercentOf', () => {
    it('读到值时钳到 0-100', () => {
        expect(hostCpuPercentOf(metricsWith({ host_cpu_percent: 42 }))).toBe(42);
        expect(hostCpuPercentOf(metricsWith({ host_cpu_percent: -3 }))).toBe(0);
        expect(hostCpuPercentOf(metricsWith({ host_cpu_percent: 120 }))).toBe(100);
    });

    it('缺字段或非有限数返回 null', () => {
        expect(hostCpuPercentOf(metricsWith(undefined))).toBeNull();
        expect(hostCpuPercentOf(metricsWith({ host_cpu_percent: Number.NaN }))).toBeNull();
    });
});

describe('hostDiskRatioOf', () => {
    it('已用除以总量得到比例', () => {
        const m = metricsWith({ host_disk_used_bytes: 30, host_disk_total_bytes: 100 });
        expect(hostDiskRatioOf(m)).toBe(0.3);
    });

    it('缺任一字段或总量非正返回 null', () => {
        expect(hostDiskRatioOf(metricsWith(undefined))).toBeNull();
        expect(
            hostDiskRatioOf(metricsWith({ host_disk_used_bytes: 30, host_disk_total_bytes: 0 })),
        ).toBeNull();
    });
});

describe('rssRatioOfHostTotal', () => {
    it('进程 RSS 相对主机总量得到比例', () => {
        const m = metricsWith({ rss_bytes: 2, host_total_bytes: 16 });
        expect(rssRatioOfHostTotal(m)).toBe(0.125);
    });

    it('缺 RSS 或总量非正返回 null', () => {
        expect(rssRatioOfHostTotal(metricsWith({ host_total_bytes: 16 }))).toBeNull();
        expect(rssRatioOfHostTotal(metricsWith({ rss_bytes: 2, host_total_bytes: 0 }))).toBeNull();
    });
});

describe('activeNodesCountOf', () => {
    it('只统计有活动时间的节点', () => {
        expect(activeNodesCountOf([node(1), node(0), node(undefined), node(9)])).toBe(2);
    });

    it('节点列表缺失按 0 处理', () => {
        expect(activeNodesCountOf(undefined)).toBe(0);
        expect(activeNodesCountOf(null)).toBe(0);
    });
});

describe('probeBadgeTone', () => {
    it('探针状态映射到徽章色调', () => {
        expect(probeBadgeTone('active')).toBe('success');
        expect(probeBadgeTone('error')).toBe('danger');
        expect(probeBadgeTone('stale')).toBe('warning');
        expect(probeBadgeTone('not_injected')).toBe('neutral');
        expect(probeBadgeTone(undefined)).toBe('neutral');
    });
});

describe('historySeriesTitle', () => {
    it('三个序列各有固定标题', () => {
        expect(historySeriesTitle('rss')).toBe('内存 RSS');
        expect(historySeriesTitle('eventsOut')).toBe('出站事件');
        expect(historySeriesTitle('actionsIn')).toBe('入站 action');
    });
});
