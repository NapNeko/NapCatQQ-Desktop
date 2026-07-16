// 列表卡 meta 下方：实例运行时指标一行摘要；点击进入指标全页。

import {
    formatBytes,
    formatCompactCount,
    rssBytesOf,
    sumNodeTotals,
} from '../../../../core/domain/bot/runtime-metrics-settings';
import type { BotRuntimeMetrics } from '../../../../core/ipc/generated/domain/BotRuntimeMetrics';
import { cn } from '../../../../shared/utils/cn';
import { AlertCircle, ChevronRight, Clock3 } from 'lucide-react';

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
        const text =
            metrics.probe === 'error'
                ? `指标异常${metrics.probe_error ? ` · ${metrics.probe_error}` : ''}`
                : metrics.probe_error
                    ? `未注入 · ${metrics.probe_error}`
                    : '指标未注入 · 重启后生效';
        const Icon = metrics.probe === 'error' ? AlertCircle : Clock3;
        if (!clickable) {
            return <p className="mt-1.5 truncate text-[11px] text-text-tertiary">{text}</p>;
        }
        return (
            <button
                type="button"
                aria-label={`查看运行时指标：${text}`}
                title={text}
                className={cn(
                    'mt-1 flex h-6 max-w-full items-center gap-1.5 rounded-sm px-1.5 text-[11.5px]',
                    'transition-[color,background-color] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/40',
                    metrics.probe === 'error'
                        ? 'text-danger hover:bg-danger-soft/35'
                        : 'text-text-tertiary hover:bg-inset/45 hover:text-text-secondary',
                )}
                onClick={(e) => {
                    e.stopPropagation();
                    onOpenDetail();
                }}
            >
                <Icon aria-hidden size={12} strokeWidth={2.2} className="shrink-0" />
                <span className="truncate">{text}</span>
            </button>
        );
    }

    const rss = rssBytesOf(metrics);
    const totals = sumNodeTotals(metrics.nodes);
    const line = `内存 ${formatBytes(rss)}，出站 ${formatCompactCount(totals.eventsOut)}，入站 ${formatCompactCount(totals.actionsIn)}${metrics.probe === 'stale' ? '，数据陈旧' : ''}`;

    if (!clickable) {
        return <p className="mt-1.5 truncate font-mono text-[11px] text-text-secondary">{line}</p>;
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
                'mt-1 flex h-6 max-w-full items-center gap-1.5 rounded-sm px-1.5 text-[11.5px] text-text-tertiary',
                'transition-[color,background-color] hover:bg-inset/45 hover:text-text-secondary',
                'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/40',
            )}
        >
            <span
                aria-hidden
                className={cn(
                    'h-1.5 w-1.5 shrink-0 rounded-full',
                    metrics.probe === 'stale' ? 'bg-warning' : 'bg-brand',
                )}
            />
            <span className="flex min-w-0 items-center gap-1.5 font-mono tabular-nums">
                <span className="truncate">RSS {formatBytes(rss)}</span>
                <span aria-hidden className="text-border">·</span>
                <span className="shrink-0">流量 {formatCompactCount(totals.eventsOut)} / {formatCompactCount(totals.actionsIn)}</span>
            </span>
            {metrics.probe === 'stale' ? (
                <Clock3 aria-hidden size={11} className="shrink-0 text-warning" />
            ) : (
                <ChevronRight aria-hidden size={12} className="shrink-0 text-text-disabled" />
            )}
        </button>
    );
}
