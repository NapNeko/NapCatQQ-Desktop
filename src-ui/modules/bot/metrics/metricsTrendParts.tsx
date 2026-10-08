// 趋势图周边件：分段选择器、显示配置菜单、流量趋势卡（历史曲线 + 空态/失败态）

import { type ReactNode } from 'react';
import { Activity, ChevronDown, Settings2 } from 'lucide-react';
import {
    BotRuntimeMetricsHistoryChart,
    type HistorySeriesKey,
} from './BotRuntimeMetricsHistoryChart';
import { Panel } from './metricsPanelParts';
import type { MetricsHistoryPoint } from '../../../core/ipc/generated/domain/MetricsHistoryPoint';
import type { MetricsNodeTotals } from '../../../core/domain/bot/runtime-metrics-settings';
import { formatBytes, formatCompactCount } from '../../../core/domain/bot/runtime-metrics-settings';
import { historySeriesTitle } from '../../../core/domain/bot/runtime-metrics-display';
import {
    Button,
    Popover,
    PopoverContent,
    PopoverTrigger,
    Select,
    Switch,
} from '../../../shared/ui';
import { cn } from '../../../shared/utils/cn';

const SERIES_OPTIONS: { id: HistorySeriesKey; label: string }[] = [
    { id: 'rss', label: '内存 RSS' },
    { id: 'eventsOut', label: '出站事件' },
    { id: 'actionsIn', label: '入站 action' },
];

export function SegmentControl<T extends string>({
    value,
    onChange,
    items,
    ariaLabel,
}: {
    value: T;
    onChange: (next: T) => void;
    items: ReadonlyArray<{ value: T; label: string; title?: string }>;
    ariaLabel: string;
}) {
    return (
        <div
            className="flex h-7 w-full items-center rounded-md bg-inset p-0.5"
            role="group"
            aria-label={ariaLabel}
        >
            {items.map((it) => {
                const selected = value === it.value;
                return (
                    <button
                        key={it.value}
                        type="button"
                        aria-pressed={selected}
                        title={it.title}
                        onClick={() => onChange(it.value)}
                        className={cn(
                            'flex h-6 min-w-0 flex-1 items-center justify-center rounded-sm px-2 text-[12px] font-medium transition-all',
                            selected
                                ? 'border border-border/50 bg-surface text-text shadow-sm'
                                : 'border border-transparent text-text-tertiary hover:text-text',
                        )}
                    >
                        {it.label}
                    </button>
                );
            })}
        </div>
    );
}

export function TrendConfigMenu({
    series,
    onSeriesChange,
    fitData,
    onFitDataChange,
    showDots,
    onShowDotsChange,
}: {
    series: HistorySeriesKey;
    onSeriesChange: (v: HistorySeriesKey) => void;
    fitData: boolean;
    onFitDataChange: (v: boolean) => void;
    showDots: boolean;
    onShowDotsChange: (v: boolean) => void;
}) {
    const seriesLabel = SERIES_OPTIONS.find((o) => o.id === series)?.label ?? '序列';

    return (
        <Popover>
            <PopoverTrigger asChild>
                <button
                    type="button"
                    className="inline-flex h-7 items-center gap-1 rounded-md bg-inset px-2 text-[11px] font-medium text-text-secondary transition-colors hover:bg-muted/50 hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/40"
                    aria-label="趋势显示配置"
                    title="序列、纵轴与采样点"
                >
                    <Settings2 aria-hidden size={13} strokeWidth={2.1} />
                    <span className="max-w-[5.5rem] truncate">{seriesLabel}</span>
                    <ChevronDown aria-hidden size={12} className="text-text-tertiary" />
                </button>
            </PopoverTrigger>
            <PopoverContent side="bottom" align="end" sideOffset={6} className="w-[17rem] p-3">
                <div className="space-y-3">
                    <Select
                        label="显示序列"
                        value={series}
                        onValueChange={onSeriesChange}
                        items={SERIES_OPTIONS.map((o) => ({
                            value: o.id,
                            label: o.label,
                        }))}
                    />

                    <div className="space-y-1.5">
                        <p className="text-xs font-medium text-text-secondary">纵轴</p>
                        <SegmentControl
                            ariaLabel="纵轴模式"
                            value={fitData ? 'fit' : 'zero'}
                            onChange={(v) => onFitDataChange(v === 'fit')}
                            items={[
                                {
                                    value: 'zero',
                                    label: '从 0',
                                },
                                {
                                    value: 'fit',
                                    label: '贴合',
                                    title: '贴合数据区间看小波动',
                                },
                            ]}
                        />
                    </div>

                    <div className="border-t border-border-subtle/70 pt-2.5">
                        <Switch
                            checked={showDots}
                            onCheckedChange={onShowDotsChange}
                            label="显示采样点"
                            hint="点过多时会自动抽稀"
                        />
                    </div>
                </div>
            </PopoverContent>
        </Popover>
    );
}

// 趋势卡：三个流量小结 + 历史曲线；首拉 / 失败 / 空态都在卡内自管
export function TrendChartCard({
    aside,
    totals,
    points,
    loading,
    error,
    onRetry,
    series,
    fitData,
    showDots,
}: {
    aside: ReactNode;
    totals: MetricsNodeTotals;
    points: MetricsHistoryPoint[];
    loading: boolean;
    error: string | null;
    onRetry: () => Promise<void>;
    series: HistorySeriesKey;
    fitData: boolean;
    showDots: boolean;
}) {
    return (
        <Panel
            title="流量趋势"
            description="约每分钟一点"
            className="min-h-[18rem] lg:col-span-5 lg:row-span-2"
            aside={aside}
        >
            <div className="flex h-full min-h-0 flex-col gap-2">
                <div className="grid shrink-0 grid-cols-3 gap-1.5">
                    <div className="rounded-sm bg-inset/45 px-2 py-1.5 text-center">
                        <p className="text-[10px] text-text-tertiary">出站事件</p>
                        <p className="mt-0.5 font-mono text-sm font-semibold tabular-nums text-text">
                            {formatCompactCount(totals.eventsOut)}
                        </p>
                    </div>
                    <div className="rounded-sm bg-inset/45 px-2 py-1.5 text-center">
                        <p className="text-[10px] text-text-tertiary">入站 action</p>
                        <p className="mt-0.5 font-mono text-sm font-semibold tabular-nums text-text">
                            {formatCompactCount(totals.actionsIn)}
                        </p>
                    </div>
                    <div className="rounded-sm bg-inset/45 px-2 py-1.5 text-center">
                        <p className="text-[10px] text-text-tertiary">字节 出/入</p>
                        <p className="mt-0.5 truncate font-mono text-[11px] font-semibold tabular-nums text-text">
                            {formatBytes(totals.bytesOut)} / {formatBytes(totals.bytesIn)}
                        </p>
                    </div>
                </div>

                <div className="min-h-0 flex-1">
                    {loading && points.length === 0 ? (
                        <div className="flex h-full items-center justify-center rounded-sm bg-inset/35 text-2xs text-text-tertiary">
                            加载历史…
                        </div>
                    ) : error ? (
                        <div className="flex h-full flex-col items-center justify-center gap-1.5 rounded-sm bg-danger-soft/25 px-3 text-center">
                            <p className="text-2xs font-medium text-danger">历史读取失败</p>
                            <Button type="button" variant="ghost" size="sm" onClick={onRetry}>
                                重试
                            </Button>
                        </div>
                    ) : points.length === 0 ? (
                        <div className="flex h-full flex-col items-center justify-center rounded-sm bg-inset/35 px-3 text-center">
                            <Activity aria-hidden size={16} className="mb-1 text-text-disabled" />
                            <p className="text-2xs text-text-tertiary">尚无历史采样点</p>
                        </div>
                    ) : (
                        <BotRuntimeMetricsHistoryChart
                            points={points}
                            series={series}
                            title={historySeriesTitle(series)}
                            accentColor="var(--color-brand)"
                            scaleMode={fitData ? 'fit' : 'zero'}
                            showDots={showDots}
                            className="h-full min-h-0"
                        />
                    )}
                </div>
            </div>
        </Panel>
    );
}
