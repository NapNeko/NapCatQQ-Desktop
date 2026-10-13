// 列表卡 meta 下方：实例运行时指标结构化微摘要；点击进入指标全页。

import {
    formatBytes,
    formatCompactCount,
    rssBytesOf,
    sumNodeTotals,
} from '../../../../core/domain/bot/runtime-metrics-settings';
import type { BotRuntimeMetrics } from '../../../../core/ipc/generated/domain/BotRuntimeMetrics';
import { cn } from '../../../../shared/utils/cn';
import { AlertCircle, ArrowUpDown, ChevronRight, Clock3, HardDrive } from 'lucide-react';

export interface BotRuntimeMetricsStripProps {
    metrics: BotRuntimeMetrics | null;
    enabled: boolean;
    onOpenDetail?: () => void;
}

export function BotRuntimeMetricsStrip({
    metrics,
    enabled,
    onOpenDetail,
}: BotRuntimeMetricsStripProps) {
    if (!enabled || !metrics) return null;

    const clickable = typeof onOpenDetail === 'function';

    if (metrics.probe === 'not_injected' || metrics.probe === 'error') {
        const short = metrics.probe === 'error' ? '指标异常' : '未注入';
        const detail = metrics.probe_error?.trim() || undefined;
        const Icon = metrics.probe === 'error' ? AlertCircle : Clock3;
        if (!clickable) {
            return (
                <div
                    className="flex items-center gap-1.5 text-2xs text-text-tertiary"
                    title={detail}
                >
                    <Icon
                        aria-hidden
                        size={11}
                        className={metrics.probe === 'error' ? 'text-danger' : 'text-text-tertiary'}
                    />
                    <span className="truncate">{short}</span>
                </div>
            );
        }
        return (
            <button
                type="button"
                aria-label={
                    detail ? `查看运行时指标：${short}。${detail}` : `查看运行时指标：${short}`
                }
                title={detail ?? short}
                className={cn(
                    'inline-flex h-6 max-w-full items-center gap-1.5 rounded-sm border px-2 text-2xs',
                    'transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/40',
                    metrics.probe === 'error'
                        ? 'border-danger/30 bg-danger/8 text-danger hover:bg-danger/15'
                        : 'border-border-subtle bg-inset/50 text-text-tertiary hover:bg-inset hover:text-text-secondary',
                )}
                onClick={(e) => {
                    e.stopPropagation();
                    onOpenDetail();
                }}
            >
                <Icon aria-hidden size={11} strokeWidth={2.2} className="shrink-0" />
                <span className="truncate">{short}</span>
            </button>
        );
    }

    const rss = rssBytesOf(metrics);
    const totals = sumNodeTotals(metrics.nodes);
    const line = `内存 ${formatBytes(rss)}，出站 ${formatCompactCount(totals.eventsOut)}，入站 ${formatCompactCount(totals.actionsIn)}${metrics.probe === 'stale' ? '，数据陈旧' : ''}`;

    const chips = (
        <div className="flex max-w-full flex-wrap items-center gap-1.5 font-mono text-[11px] tabular-nums">
            <span
                className="inline-flex items-center gap-1 rounded-xs border border-border-subtle/80 bg-inset/60 px-1.5 py-0.5 text-text-secondary"
                title={`物理内存占用 (RSS): ${formatBytes(rss)}`}
            >
                <HardDrive size={10.5} className="shrink-0 text-text-tertiary" />
                <span>{formatBytes(rss)}</span>
            </span>
            <span
                className="inline-flex items-center gap-1 rounded-xs border border-border-subtle/80 bg-inset/60 px-1.5 py-0.5 text-text-secondary"
                title={`事件出站 ${formatCompactCount(totals.eventsOut)} / 动作入站 ${formatCompactCount(totals.actionsIn)}`}
            >
                <ArrowUpDown size={10.5} className="shrink-0 text-text-tertiary" />
                <span>
                    ↑{formatCompactCount(totals.eventsOut)} ↓{formatCompactCount(totals.actionsIn)}
                </span>
            </span>
            {metrics.probe === 'stale' ? (
                <span
                    className="inline-flex items-center gap-1 rounded-xs border border-warning/30 bg-warning/10 px-1.5 py-0.5 text-warning"
                    title="数据陈旧"
                >
                    <Clock3 size={10} className="shrink-0" />
                    <span>陈旧</span>
                </span>
            ) : null}
        </div>
    );

    if (!clickable) {
        return chips;
    }

    return (
        <button
            type="button"
            aria-label={`查看运行时指标：${line}`}
            title={line}
            onClick={(e) => {
                e.stopPropagation();
                onOpenDetail();
            }}
            className={cn(
                'group -ml-1 inline-flex max-w-full items-center gap-1.5 rounded-sm p-1 transition-all duration-fast',
                'hover:bg-inset/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/40',
            )}
        >
            {chips}
            <ChevronRight
                aria-hidden
                size={12}
                className="shrink-0 text-text-tertiary opacity-0 transition-all duration-fast group-hover:translate-x-0.5 group-hover:opacity-100"
            />
        </button>
    );
}
