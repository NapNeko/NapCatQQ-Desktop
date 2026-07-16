// Bot 运行时指标全页：与配置 / 日志同级。
// 单页自适应仪表盘：总览 / 系统资源 / 节点流量同屏，页面本身不滚动。

import { useEffect, useMemo, useState, type ReactNode } from 'react';
import {
    Activity,
    AlertTriangle,
    ArrowLeft,
    CalendarRange,
    ChevronDown,
    CircleHelp,
    Cpu,
    Database,
    HardDrive,
    MemoryStick,
    RefreshCw,
    Settings2,
} from 'lucide-react';
import {
    applyTimeToMs,
    Badge,
    Button,
    Checkbox,
    formatTimeValue,
    MonthCalendar,
    Popover,
    PopoverContent,
    PopoverTrigger,
    Select,
    Switch,
    timeFromMs,
    TimePicker,
    Tooltip,
    TooltipContent,
    TooltipProvider,
    TooltipTrigger,
} from '../../../shared/ui';
import { ActionMotionIcon } from '../../../shared/ui/motion';
import { useBotRuntimeMetrics } from '../../../hooks/bot/useBotRuntimeMetrics';
import { useBotRuntimeMetricsHistory } from '../../../hooks/bot/useBotRuntimeMetricsHistory';
import { useBotConfigsMap } from '../../../hooks/bot/useBotConfigsMap';
import { useBotSnapshots } from '../../../hooks/bot/useBotSnapshots';
import {
    combineLocalDateAndTime,
    formatBytes,
    formatCollectedAgo,
    formatCompactCount,
    formatHistoryWindowLabel,
    formatLocalDateLabel,
    isMetricsHistoryRangeAvailable,
    METRICS_HISTORY_RANGE_OPTIONS,
    networkNodeKindLabel,
    probeHealthLabel,
    resolveHistoryWindowBounds,
    rssBytesOf,
    startOfLocalDay,
    sumNodeTotals,
    type MetricsHistoryRange,
    type MetricsHistoryWindow,
} from '../../../core/domain/bot/runtime-metrics-settings';
import { isRuntimeTargetLocal } from '../../../core/domain/bot/runtime-target';
import type { NetworkNodeMetrics } from '../../../core/ipc/generated/domain/NetworkNodeMetrics';
import { cn } from '../../../shared/utils/cn';
import {
    BotRuntimeMetricsHistoryChart,
    type HistorySeriesKey,
} from './BotRuntimeMetricsHistoryChart';

export interface BotRuntimeMetricsPageNextProps {
    botId: string;
    onBack: () => void;
}

function formatActivity(ms: number | null | undefined): string {
    if (ms == null || !Number.isFinite(Number(ms)) || Number(ms) <= 0) return '—';
    return formatCollectedAgo(Number(ms));
}

function Panel({
    title,
    description,
    aside,
    children,
    className,
}: {
    title: string;
    description?: string;
    aside?: ReactNode;
    children: ReactNode;
    className?: string;
}) {
    return (
        <section
            className={cn(
                'flex min-h-0 min-w-0 flex-col overflow-hidden rounded-md bg-elevated/40 ring-1 ring-border-subtle',
                className,
            )}
        >
            <div className="flex shrink-0 items-start justify-between gap-2 border-b border-border-subtle/70 px-3 py-2.5">
                <div className="min-w-0">
                    <div className="flex items-center gap-2">
                        <span
                            aria-hidden
                            className="h-3.5 w-0.5 shrink-0 rounded-full bg-brand/55"
                        />
                        <h2 className="truncate text-[13px] font-semibold text-text">
                            {title}
                        </h2>
                    </div>
                    {description ? (
                        <p className="mt-1 pl-2.5 text-2xs text-text-tertiary">
                            {description}
                        </p>
                    ) : null}
                </div>
                {aside ? <div className="shrink-0">{aside}</div> : null}
            </div>
            <div className="min-h-0 flex-1 overflow-hidden p-3">{children}</div>
        </section>
    );
}

function KpiTile({
    label,
    value,
    tone = 'brand',
    hint,
}: {
    label: string;
    value: string;
    tone?: 'brand' | 'success' | 'danger' | 'neutral' | 'warning';
    hint?: string;
}) {
    return (
        <div
            className="flex h-full min-h-0 min-w-0 flex-col items-center justify-center rounded-sm bg-inset/50 px-2.5 py-2 text-center"
            role="status"
            aria-label={`${label}：${value}${hint ? `，${hint}` : ''}`}
            title={hint ? `${value} · ${hint}` : value}
        >
            <div className="flex items-center justify-center gap-1.5">
                <span
                    aria-hidden
                    className={cn(
                        'h-1.5 w-1.5 shrink-0 rounded-full',
                        tone === 'brand' && 'bg-brand',
                        tone === 'success' && 'bg-success',
                        tone === 'danger' && 'bg-danger',
                        tone === 'warning' && 'bg-warning',
                        tone === 'neutral' && 'bg-text-disabled',
                    )}
                />
                <p className="text-[10.5px] font-medium leading-none text-text-tertiary">
                    {label}
                </p>
            </div>
            <p
                className={cn(
                    'mt-1.5 max-w-full truncate font-mono text-[clamp(1.05rem,2.4cqi,1.55rem)] font-semibold leading-none tabular-nums',
                    tone === 'danger' ? 'text-danger' : 'text-text',
                )}
            >
                {value}
            </p>
            {hint ? (
                <p className="mt-1 max-w-full truncate text-[10px] leading-none text-text-tertiary">
                    {hint}
                </p>
            ) : null}
        </div>
    );
}

function ResourceMeter({
    icon: Icon,
    label,
    value,
    detail,
    ratio,
    unavailable,
}: {
    icon: typeof MemoryStick;
    label: string;
    value: string;
    detail?: string;
    ratio?: number | null;
    unavailable?: boolean;
}) {
    const pct =
        ratio != null && Number.isFinite(ratio)
            ? Math.max(0, Math.min(100, ratio * 100))
            : null;

    return (
        <div
            className={cn(
                'flex min-h-0 flex-1 flex-col justify-center rounded-sm bg-inset/45 px-3 py-2.5',
                unavailable && 'opacity-75',
            )}
        >
            <div className="flex items-center gap-2">
                <span className="inline-flex h-7 w-7 items-center justify-center rounded-sm bg-surface/70 text-brand ring-1 ring-border-subtle">
                    <Icon aria-hidden size={14} strokeWidth={2.1} />
                </span>
                <div className="min-w-0 flex-1">
                    <div className="flex items-baseline justify-between gap-2">
                        <p className="text-2xs font-medium text-text-tertiary">{label}</p>
                        <p className="truncate font-mono text-sm font-semibold tabular-nums text-text">
                            {value}
                        </p>
                    </div>
                    {detail ? (
                        <p className="mt-0.5 truncate text-2xs text-text-tertiary">
                            {detail}
                        </p>
                    ) : null}
                </div>
            </div>
            <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-inset">
                {pct != null ? (
                    <div
                        className="h-full rounded-full bg-brand/80 transition-[width]"
                        style={{ width: `${pct}%` }}
                    />
                ) : (
                    <div className="h-full w-full rounded-full bg-border-subtle/60" />
                )}
            </div>
        </div>
    );
}

const SERIES_OPTIONS: { id: HistorySeriesKey; label: string; hint: string }[] = [
    { id: 'rss', label: '内存 RSS', hint: '进程常驻内存趋势' },
    { id: 'eventsOut', label: '出站事件', hint: 'OneBot 事件发出累计' },
    { id: 'actionsIn', label: '入站 action', hint: '收到的 action 累计' },
];

function SegmentControl<T extends string>({
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
                            'flex h-6 min-w-0 flex-1 items-center justify-center rounded-sm px-2 text-[12px] font-medium transition-colors',
                            selected
                                ? 'bg-surface text-text shadow-[0_1px_2px_rgba(0,0,0,0.04)]'
                                : 'text-text-tertiary hover:text-text',
                        )}
                    >
                        {it.label}
                    </button>
                );
            })}
        </div>
    );
}

function TrendConfigMenu({
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
    const seriesLabel =
        SERIES_OPTIONS.find((o) => o.id === series)?.label ?? '序列';

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
                        hint={
                            SERIES_OPTIONS.find((o) => o.id === series)?.hint ??
                            undefined
                        }
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
                                    title: '从 0 起，便于读绝对量',
                                },
                                {
                                    value: 'fit',
                                    label: '贴合',
                                    title: '贴合数据区间，便于看小波动',
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

function TrendRangePanel({
    window,
    onChange,
    retentionDays,
}: {
    window: MetricsHistoryWindow;
    onChange: (next: MetricsHistoryWindow) => void;
    retentionDays: number;
}) {
    const [open, setOpen] = useState(false);
    const now = Date.now();
    const bounds = resolveHistoryWindowBounds(window, retentionDays, now);
    const minDayMs = startOfLocalDay(now - retentionDays * 86400_000);
    const maxDayMs = startOfLocalDay(now);

    const [draftFromMs, setDraftFromMs] = useState(bounds.fromMs);
    const [draftToMs, setDraftToMs] = useState(bounds.toMs);
    const [draftFollowNow, setDraftFollowNow] = useState(
        window.mode === 'custom' ? window.followNow : true,
    );
    const [activeField, setActiveField] = useState<'from' | 'to'>('from');
    const [monthCursor, setMonthCursor] = useState(
        () => new Date(startOfLocalDay(bounds.fromMs)),
    );
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        if (!open) return;
        const b = resolveHistoryWindowBounds(window, retentionDays);
        setDraftFromMs(b.fromMs);
        setDraftToMs(b.toMs);
        setDraftFollowNow(window.mode === 'custom' ? window.followNow : true);
        setActiveField('from');
        setMonthCursor(new Date(startOfLocalDay(b.fromMs)));
        setError(null);
    }, [open, window, retentionDays]);

    // 跟随当前时刻时，面板打开期间刷新结束时间
    useEffect(() => {
        if (!open || !draftFollowNow) return;
        const tick = () => setDraftToMs(Date.now());
        tick();
        const id = globalThis.setInterval(tick, 1000);
        return () => globalThis.clearInterval(id);
    }, [open, draftFollowNow]);

    const applyPreset = (id: MetricsHistoryRange) => {
        onChange({ mode: 'preset', range: id });
        setOpen(false);
    };

    const onSelectDay = (dayStartMs: number) => {
        setError(null);
        if (draftFollowNow || activeField === 'from') {
            const next = combineLocalDateAndTime(dayStartMs, draftFromMs);
            setDraftFromMs(next);
            if (next > draftToMs) setDraftToMs(next);
            if (!draftFollowNow) setActiveField('to');
            return;
        }
        const next = combineLocalDateAndTime(dayStartMs, draftToMs);
        if (next < draftFromMs) {
            setDraftFromMs(next);
            setActiveField('to');
        } else {
            setDraftToMs(next);
        }
    };

    const apply = () => {
        if (draftFromMs > draftToMs) {
            setError('开始时间不能晚于结束时间');
            return;
        }
        onChange({
            mode: 'custom',
            fromMs: draftFromMs,
            toMs: draftFollowNow ? Date.now() : draftToMs,
            followNow: draftFollowNow,
        });
        setOpen(false);
    };

    const label = formatHistoryWindowLabel(window);
    const presetActive =
        window.mode === 'preset' ? window.range : null;

    const renderField = (field: 'from' | 'to') => {
        const isEndLive = field === 'to' && draftFollowNow;
        const ts = field === 'from' ? draftFromMs : draftToMs;
        const setTs = field === 'from' ? setDraftFromMs : setDraftToMs;
        const isActive = activeField === field && !isEndLive;
        const t = timeFromMs(ts);

        return (
            <div
                role="button"
                tabIndex={isEndLive ? -1 : 0}
                onClick={() => {
                    if (isEndLive) return;
                    setActiveField(field);
                    setMonthCursor(new Date(startOfLocalDay(ts)));
                }}
                onKeyDown={(e) => {
                    if (isEndLive) return;
                    if (e.key === 'Enter' || e.key === ' ') {
                        e.preventDefault();
                        setActiveField(field);
                        setMonthCursor(new Date(startOfLocalDay(ts)));
                    }
                }}
                className={cn(
                    'rounded-md border px-3 py-2.5 transition-colors',
                    isEndLive
                        ? 'cursor-not-allowed border-border-subtle/50 bg-inset/40 opacity-55'
                        : isActive
                            ? 'cursor-pointer border-brand bg-brand-soft/20 ring-1 ring-brand/30'
                            : 'cursor-pointer border-border-subtle bg-field hover:border-border',
                )}
            >
                <p className="mb-1.5 text-[10px] font-medium tracking-wide text-text-tertiary">
                    {field === 'from' ? '开始时间' : '结束时间'}
                </p>
                <div className="flex min-w-0 items-center gap-2">
                    <span className="min-w-0 flex-1 font-mono text-[12px] tabular-nums text-text">
                        {formatLocalDateLabel(ts)}
                    </span>
                    <span className="font-mono text-[12px] tabular-nums text-text-secondary">
                        {formatTimeValue(t.hours, t.minutes)}
                    </span>
                    <TimePicker
                        hours={t.hours}
                        minutes={t.minutes}
                        disabled={isEndLive}
                        aria-label={
                            field === 'from' ? '选择开始时刻' : '选择结束时刻'
                        }
                        onChange={(next) => {
                            if (isEndLive) return;
                            const nextMs = applyTimeToMs(ts, next);
                            setTs(nextMs);
                            setActiveField(field);
                            setError(null);
                            if (field === 'from' && nextMs > draftToMs) {
                                setDraftToMs(nextMs);
                            }
                            if (field === 'to' && nextMs < draftFromMs) {
                                setDraftFromMs(nextMs);
                            }
                        }}
                    />
                </div>
                <p className="mt-1 text-[10px] text-text-tertiary">
                    {isEndLive
                        ? '跟随当前时刻'
                        : isActive
                            ? '在右侧月历改日期 · 点时钟改时刻'
                            : '点此编辑'}
                </p>
            </div>
        );
    };

    return (
        <Popover open={open} onOpenChange={setOpen}>
            <PopoverTrigger asChild>
                <button
                    type="button"
                    className="inline-flex h-7 max-w-[12rem] items-center gap-1 rounded-md bg-inset px-2 text-[11px] font-medium text-text-secondary transition-colors hover:bg-muted/50 hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/40"
                    aria-label="时间颗粒度"
                    title={label}
                >
                    <CalendarRange aria-hidden size={13} strokeWidth={2.1} />
                    <span className="min-w-0 truncate">{label}</span>
                    <ChevronDown aria-hidden size={12} className="shrink-0 text-text-tertiary" />
                </button>
            </PopoverTrigger>
            <PopoverContent
                side="bottom"
                align="end"
                sideOffset={6}
                // 宽面板 + 容器查询布局（见 index.css .ndf-metrics-range-*）
                className="ndf-metrics-range-popover w-[min(38.75rem,calc(100vw-1.5rem))] max-w-[calc(100vw-1.5rem)] p-3"
                // TimePicker 也是 Popover portal：点时钟时事件在外层面板外，
                // 不拦截会把整个时间面板关掉，右侧月历像「被关掉」一样消失。
                onInteractOutside={(e) => {
                    const t = e.target as HTMLElement | null;
                    if (t?.closest?.('[data-radix-popper-content-wrapper]')) {
                        e.preventDefault();
                    }
                }}
                onFocusOutside={(e) => {
                    const t = e.target as HTMLElement | null;
                    if (t?.closest?.('[data-radix-popper-content-wrapper]')) {
                        e.preventDefault();
                    }
                }}
            >
                {/* 快捷范围：顶栏横排，点即应用 */}
                <div
                    className="mb-3 flex flex-wrap gap-1.5 border-b border-border-subtle/70 pb-2.5"
                    role="group"
                    aria-label="快捷时间范围"
                >
                    {METRICS_HISTORY_RANGE_OPTIONS.map((opt) => {
                        const ok = isMetricsHistoryRangeAvailable(
                            opt.id,
                            retentionDays,
                        );
                        const selected = presetActive === opt.id;
                        return (
                            <button
                                key={opt.id}
                                type="button"
                                disabled={!ok}
                                aria-pressed={selected}
                                onClick={() => applyPreset(opt.id)}
                                className={cn(
                                    'h-7 rounded-sm px-2.5 text-[12px] font-medium transition-colors',
                                    selected
                                        ? 'bg-brand text-white shadow-sm'
                                        : 'bg-inset text-text-tertiary hover:bg-elevated hover:text-text',
                                    !ok && 'cursor-not-allowed opacity-40',
                                )}
                                title={
                                    ok
                                        ? opt.label
                                        : `保留 ${retentionDays} 天，无法选 ${opt.label}`
                                }
                            >
                                {opt.shortLabel}
                            </button>
                        );
                    })}
                </div>

                {/* 左字段 + 右月历（未关闭；宽时并排，窄时月历在下方） */}
                <div className="ndf-metrics-range-layout">
                    <div className="ndf-metrics-range-fields">
                        <p className="shrink-0 text-[11px] leading-relaxed text-text-tertiary">
                            支持日期与时间 · 最长保留 {retentionDays} 天
                        </p>
                        {renderField('from')}
                        {renderField('to')}

                        {/* CCS 同款：复选框，不是 Switch */}
                        <Checkbox
                            checked={draftFollowNow}
                            onCheckedChange={(on) => {
                                setDraftFollowNow(on);
                                setError(null);
                                if (on) {
                                    setDraftToMs(Date.now());
                                    setActiveField('from');
                                } else {
                                    setActiveField('to');
                                }
                            }}
                            label="结束时间跟随当前时刻"
                        />

                        {error ? (
                            <p className="text-[11px] text-danger">{error}</p>
                        ) : null}

                        <div className="mt-auto flex gap-1.5 pt-1">
                            <Button
                                type="button"
                                variant="ghost"
                                size="sm"
                                className="flex-1"
                                onClick={() => setOpen(false)}
                            >
                                取消
                            </Button>
                            <Button
                                type="button"
                                size="sm"
                                className="flex-1"
                                onClick={apply}
                            >
                                确定
                            </Button>
                        </div>
                    </div>

                    <div className="ndf-metrics-range-calendar">
                        <MonthCalendar
                            month={monthCursor}
                            onMonthChange={setMonthCursor}
                            rangeStartMs={draftFromMs}
                            rangeEndMs={draftToMs}
                            onSelectDay={onSelectDay}
                            minDayMs={minDayMs}
                            maxDayMs={maxDayMs}
                        />
                    </div>
                </div>
            </PopoverContent>
        </Popover>
    );
}

function NodeRows({ nodes }: { nodes: NetworkNodeMetrics[] }) {
    if (nodes.length === 0) {
        return (
            <div className="flex h-full min-h-0 flex-col items-center justify-center px-3 text-center">
                <Database aria-hidden size={18} className="mb-1.5 text-text-disabled" />
                <p className="text-xs font-medium text-text-secondary">暂无节点数据</p>
                <p className="mt-1 max-w-xs text-2xs leading-relaxed text-text-tertiary">
                    配置 OneBot 连接并产生流量后显示
                </p>
            </div>
        );
    }

    return (
        <div className="h-full min-h-0 overflow-auto rounded-sm bg-field ring-1 ring-border-subtle">
            <table className="w-full border-collapse text-left text-2xs">
                <thead className="sticky top-0 z-[1] bg-field text-text-tertiary">
                    <tr className="border-b border-border-subtle">
                        <th className="px-2.5 py-1.5 font-medium">节点</th>
                        <th className="px-2.5 py-1.5 font-medium">类型</th>
                        <th className="px-2.5 py-1.5 font-medium tabular-nums">出/入</th>
                        <th className="px-2.5 py-1.5 font-medium tabular-nums">字节</th>
                        <th className="px-2.5 py-1.5 font-medium tabular-nums">错误</th>
                        <th className="px-2.5 py-1.5 font-medium">活动</th>
                    </tr>
                </thead>
                <tbody>
                    {nodes.map((n) => {
                        const errors = Number(n.errors ?? 0);
                        return (
                            <tr
                                key={`${n.name}-${n.kind}`}
                                className="border-b border-border-subtle/70 last:border-0 hover:bg-inset/35"
                            >
                                <td
                                    className="max-w-[8rem] truncate px-2.5 py-1.5 font-medium text-text"
                                    title={n.name || undefined}
                                >
                                    {n.name || '—'}
                                </td>
                                <td className="whitespace-nowrap px-2.5 py-1.5 text-text-secondary">
                                    {networkNodeKindLabel(n.kind)}
                                </td>
                                <td className="whitespace-nowrap px-2.5 py-1.5 font-mono tabular-nums text-text-secondary">
                                    {formatCompactCount(Number(n.events_out ?? 0))} /{' '}
                                    {formatCompactCount(Number(n.actions_in ?? 0))}
                                </td>
                                <td className="whitespace-nowrap px-2.5 py-1.5 font-mono tabular-nums text-text-secondary">
                                    {formatBytes(Number(n.bytes_out ?? 0))} /{' '}
                                    {formatBytes(Number(n.bytes_in ?? 0))}
                                </td>
                                <td
                                    className={cn(
                                        'px-2.5 py-1.5 font-mono tabular-nums',
                                        errors > 0
                                            ? 'font-semibold text-danger'
                                            : 'text-text-secondary',
                                    )}
                                >
                                    {formatCompactCount(errors)}
                                </td>
                                <td className="whitespace-nowrap px-2.5 py-1.5 text-text-tertiary">
                                    {formatActivity(
                                        n.last_activity_at_ms != null
                                            ? Number(n.last_activity_at_ms)
                                            : null,
                                    )}
                                </td>
                            </tr>
                        );
                    })}
                </tbody>
            </table>
        </div>
    );
}

export function BotRuntimeMetricsPageNext({
    botId,
    onBack,
}: BotRuntimeMetricsPageNextProps) {
    const { data: bots = [] } = useBotSnapshots();
    const configByBot = useBotConfigsMap(bots);
    const config = configByBot[botId] ?? null;
    const displayName =
        config?.bot.name && config.bot.name.trim().length > 0
            ? config.bot.name.trim()
            : botId;
    const flavor = config?.bot.backend_type ?? null;
    const isRemote =
        config?.bot.runtime_target != null &&
        !isRuntimeTargetLocal(config.bot.runtime_target);

    const { enabled, metrics, loading, retentionDays, refresh } =
        useBotRuntimeMetrics(botId, { liveDetail: true });

    const [historyWindow, setHistoryWindow] = useState<MetricsHistoryWindow>({
        mode: 'preset',
        range: '1h',
    });
    const [series, setSeries] = useState<HistorySeriesKey>('rss');
    /** false = 从 0 起（默认更直观）；true = 贴合数据区间看小波动 */
    const [fitData, setFitData] = useState(false);
    const [showDots, setShowDots] = useState(false);
    const history = useBotRuntimeMetricsHistory(
        botId,
        historyWindow,
        retentionDays,
        true,
    );

    useEffect(() => {
        if (historyWindow.mode !== 'preset') return;
        if (!isMetricsHistoryRangeAvailable(historyWindow.range, retentionDays)) {
            setHistoryWindow({ mode: 'preset', range: '1h' });
        }
    }, [historyWindow, retentionDays]);

    const totals = sumNodeTotals(metrics?.nodes);
    const rss = rssBytesOf(metrics);
    const heap = metrics?.memory?.heap_used_bytes;
    const hostUsed = metrics?.memory?.host_used_bytes;
    const hostTotal = metrics?.memory?.host_total_bytes;
    const hostRatio =
        hostUsed != null &&
            hostTotal != null &&
            Number(hostTotal) > 0 &&
            Number.isFinite(Number(hostUsed))
            ? Number(hostUsed) / Number(hostTotal)
            : null;

    const probe = metrics?.probe ?? 'not_injected';
    const showInjectHint = probe === 'not_injected' || probe === 'error';
    const activeNodes = useMemo(
        () =>
            (metrics?.nodes ?? []).filter(
                (node) => Number(node.last_activity_at_ms ?? 0) > 0,
            ).length,
        [metrics?.nodes],
    );

    const onRefreshAll = () => {
        refresh();
        history.refresh();
    };

    const statusBanner =
        !enabled ? (
            <div className="rounded-sm bg-inset/50 px-3 py-2 text-2xs leading-relaxed text-text-secondary ring-1 ring-border-subtle">
                实例指标未启用。请到「设置 · 监控」打开并保存，然后重启该 Bot。
            </div>
        ) : showInjectHint ? (
            <div
                className={cn(
                    'rounded-sm px-3 py-2 text-2xs leading-relaxed ring-1',
                    probe === 'error'
                        ? 'bg-danger-soft/35 text-text-secondary ring-danger/20'
                        : 'bg-warning-soft/35 text-text-secondary ring-warning/20',
                )}
            >
                <div className="flex gap-2">
                    <AlertTriangle
                        aria-hidden
                        size={14}
                        className={cn(
                            'mt-0.5 shrink-0',
                            probe === 'error' ? 'text-danger' : 'text-warning',
                        )}
                    />
                    <div className="min-w-0">
                        <p className="font-medium text-text">
                            {probe === 'error'
                                ? '暂时无法读取运行时指标'
                                : '探针尚未载入此 Bot'}
                        </p>
                        <p className="mt-0.5 text-text-tertiary">
                            {probe === 'error'
                                ? isRemote
                                    ? '检查远端连接、ncd-watch 同步与探针注入。'
                                    : '可尝试重启实例；持续失败请查 Desktop 日志。'
                                : isRemote
                                    ? '开启指标并同步 ncd-watch 后，在该机重启 Bot。'
                                    : '设置 · 监控启用并保存后，重启该实例。'}
                        </p>
                    </div>
                </div>
            </div>
        ) : null;

    const overviewHelpText = isRemote
        ? '远端历史由同机 ncd-watch 续写，Desktop 退出后不会中断。'
        : '本机历史由 Desktop 在线时写入；关闭应用后暂停采样。';

    const overviewAside = (
        <TooltipProvider delayDuration={120}>
            <Tooltip>
                <TooltipTrigger asChild>
                    <button
                        type="button"
                        title={overviewHelpText}
                        className="inline-flex h-7 w-7 items-center justify-center rounded-sm text-text-tertiary transition-colors hover:bg-inset hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/40"
                        aria-label="指标说明"
                    >
                        <CircleHelp aria-hidden size={15} strokeWidth={2.1} />
                    </button>
                </TooltipTrigger>
                <TooltipContent
                    side="bottom"
                    sideOffset={8}
                    className="max-w-[17rem] text-left font-normal leading-relaxed"
                >
                    {overviewHelpText}
                </TooltipContent>
            </Tooltip>
        </TooltipProvider>
    );

    return (
        <div className="flex h-full min-h-0 w-full flex-col overflow-hidden">
            <header className="flex shrink-0 items-center justify-between gap-3 border-b border-border-subtle py-2.5">
                <div className="flex min-w-0 items-center gap-2.5">
                    <Button
                        variant="ghost"
                        size="icon"
                        onClick={onBack}
                        aria-label="返回列表"
                    >
                        <ActionMotionIcon icon={ArrowLeft} size={16} />
                    </Button>
                    <div className="min-w-0">
                        <div className="flex min-w-0 flex-wrap items-center gap-1.5">
                            <h1 className="truncate font-display text-[15px] font-semibold text-text">
                                运行时指标
                            </h1>
                            <Badge
                                tone={
                                    probe === 'active'
                                        ? 'success'
                                        : probe === 'error'
                                            ? 'danger'
                                            : probe === 'stale'
                                                ? 'warning'
                                                : 'neutral'
                                }
                                appearance="soft"
                            >
                                {probeHealthLabel(probe)}
                            </Badge>
                            {isRemote ? (
                                <Badge tone="info" appearance="soft">
                                    远端
                                </Badge>
                            ) : null}
                            {!enabled ? (
                                <Badge tone="neutral" appearance="soft">
                                    未启用
                                </Badge>
                            ) : null}
                        </div>
                        <p className="mt-0.5 flex min-w-0 flex-wrap items-center gap-x-1.5 text-2xs text-text-tertiary">
                            <span className="truncate font-medium text-text-secondary">
                                {displayName}
                            </span>
                            <span aria-hidden className="text-border">
                                ·
                            </span>
                            <span className="font-mono tabular-nums">QQ {botId}</span>
                            {flavor ? (
                                <>
                                    <span aria-hidden className="text-border">
                                        ·
                                    </span>
                                    <span>{flavor}</span>
                                </>
                            ) : null}
                            <span aria-hidden className="text-border">
                                ·
                            </span>
                            <span className="tabular-nums">
                                {loading && !metrics
                                    ? '加载中…'
                                    : formatCollectedAgo(metrics?.collected_at_ms)}
                            </span>
                            {metrics ? (
                                <>
                                    <span aria-hidden className="text-border">
                                        ·
                                    </span>
                                    <span>
                                        {metrics.source}
                                        {isRemote ? ' · 远端' : ' · 本机'}
                                    </span>
                                </>
                            ) : null}
                        </p>
                    </div>
                </div>
                <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    disabled={loading}
                    onClick={onRefreshAll}
                >
                    <RefreshCw
                        aria-hidden
                        size={14}
                        className={cn('mr-1.5', loading && 'animate-spin')}
                    />
                    {loading ? '刷新中…' : '刷新'}
                </Button>
            </header>

            <div className="grid min-h-0 flex-1 grid-cols-1 gap-2.5 overflow-hidden pt-2.5 lg:grid-cols-12 lg:grid-rows-2">
                <div className="grid min-h-0 grid-cols-1 gap-2.5 lg:col-span-7 lg:row-span-2 lg:grid-rows-2">
                    <div className="grid min-h-0 grid-cols-1 gap-2.5 sm:grid-cols-2">
                        <Panel
                            title="总览"
                            description="当前健康度与关键计数"
                            className="min-h-0"
                            aside={overviewAside}
                        >
                            <div className="flex h-full min-h-0 flex-col gap-2">
                                {statusBanner}
                                <div className="@container min-h-0 flex-1">
                                    <div className="grid h-full min-h-0 grid-cols-2 grid-rows-2 gap-2">
                                        <KpiTile
                                            label="内存 RSS"
                                            value={formatBytes(rss)}
                                            hint={
                                                heap != null
                                                    ? `堆 ${formatBytes(Number(heap))}`
                                                    : undefined
                                            }
                                        />
                                        <KpiTile
                                            label="活跃节点"
                                            value={`${activeNodes} / ${metrics?.nodes.length ?? 0}`}
                                            tone={activeNodes > 0 ? 'success' : 'neutral'}
                                            hint={
                                                (metrics?.nodes.length ?? 0) === 0
                                                    ? '暂无节点'
                                                    : activeNodes > 0
                                                        ? '有近期活动'
                                                        : '暂无活动'
                                            }
                                        />
                                        <KpiTile
                                            label="事件 出 / 入"
                                            value={`${formatCompactCount(totals.eventsOut)} / ${formatCompactCount(totals.actionsIn)}`}
                                            hint={`${formatBytes(totals.bytesOut)} / ${formatBytes(totals.bytesIn)}`}
                                        />
                                        <KpiTile
                                            label="累计错误"
                                            value={formatCompactCount(totals.errors)}
                                            tone={totals.errors > 0 ? 'danger' : 'neutral'}
                                            hint={totals.errors > 0 ? '需关注节点表' : '正常'}
                                        />
                                    </div>
                                </div>
                            </div>
                        </Panel>

                        <Panel
                            title="系统资源"
                            description="内存可用；CPU / 磁盘待采集接入"
                            className="min-h-0"
                        >
                            <div className="flex h-full min-h-0 flex-col gap-2">
                                <ResourceMeter
                                    icon={MemoryStick}
                                    label="进程 RSS"
                                    value={formatBytes(rss)}
                                    detail={
                                        heap != null
                                            ? `堆 ${formatBytes(Number(heap))}`
                                            : '进程常驻内存'
                                    }
                                    ratio={
                                        rss != null &&
                                            hostTotal != null &&
                                            Number(hostTotal) > 0
                                            ? Number(rss) / Number(hostTotal)
                                            : null
                                    }
                                />
                                <ResourceMeter
                                    icon={HardDrive}
                                    label="主机内存"
                                    value={
                                        hostUsed != null || hostTotal != null
                                            ? `${formatBytes(hostUsed != null ? Number(hostUsed) : null)} / ${formatBytes(hostTotal != null ? Number(hostTotal) : null)}`
                                            : '—'
                                    }
                                    detail={
                                        hostRatio != null
                                            ? `占用 ${(hostRatio * 100).toFixed(1)}%`
                                            : 'WebUI / 探针未提供主机内存'
                                    }
                                    ratio={hostRatio}
                                    unavailable={hostRatio == null}
                                />
                                <ResourceMeter
                                    icon={Cpu}
                                    label="CPU"
                                    value="—"
                                    detail="当前探针未采集 CPU"
                                    unavailable
                                />
                                <ResourceMeter
                                    icon={Database}
                                    label="磁盘"
                                    value="—"
                                    detail="当前探针未采集磁盘"
                                    unavailable
                                />
                            </div>
                        </Panel>
                    </div>

                    <Panel
                        title="节点与出入流量"
                        description="各 OneBot 网络节点累计收发"
                        className="min-h-0"
                        aside={
                            <span className="font-mono text-[10px] tabular-nums text-text-tertiary">
                                出 {formatBytes(totals.bytesOut)} · 入{' '}
                                {formatBytes(totals.bytesIn)}
                            </span>
                        }
                    >
                        <NodeRows nodes={metrics?.nodes ?? []} />
                    </Panel>
                </div>

                <Panel
                    title="流量趋势"
                    description="约每分钟一点 · 悬停查看采样"
                    className="min-h-[18rem] lg:col-span-5 lg:row-span-2"
                    aside={
                        <div className="flex items-center gap-1">
                            <TrendRangePanel
                                window={historyWindow}
                                onChange={setHistoryWindow}
                                retentionDays={retentionDays}
                            />
                            <TrendConfigMenu
                                series={series}
                                onSeriesChange={setSeries}
                                fitData={fitData}
                                onFitDataChange={setFitData}
                                showDots={showDots}
                                onShowDotsChange={setShowDots}
                            />
                        </div>
                    }
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
                                    {formatBytes(totals.bytesOut)} /{' '}
                                    {formatBytes(totals.bytesIn)}
                                </p>
                            </div>
                        </div>

                        <div className="min-h-0 flex-1">
                            {history.loading && history.points.length === 0 ? (
                                <div className="flex h-full items-center justify-center rounded-sm bg-inset/35 text-2xs text-text-tertiary">
                                    加载历史…
                                </div>
                            ) : history.error ? (
                                <div className="flex h-full flex-col items-center justify-center gap-1.5 rounded-sm bg-danger-soft/25 px-3 text-center">
                                    <p className="text-2xs font-medium text-danger">
                                        历史读取失败
                                    </p>
                                    <Button
                                        type="button"
                                        variant="ghost"
                                        size="sm"
                                        onClick={history.refresh}
                                    >
                                        重试
                                    </Button>
                                </div>
                            ) : history.points.length === 0 ? (
                                <div className="flex h-full flex-col items-center justify-center rounded-sm bg-inset/35 px-3 text-center">
                                    <Activity
                                        aria-hidden
                                        size={16}
                                        className="mb-1 text-text-disabled"
                                    />
                                    <p className="text-2xs text-text-tertiary">
                                        尚无历史采样点
                                    </p>
                                </div>
                            ) : (
                                <BotRuntimeMetricsHistoryChart
                                    points={history.points}
                                    series={series}
                                    title={
                                        series === 'rss'
                                            ? '内存 RSS'
                                            : series === 'eventsOut'
                                                ? '出站事件'
                                                : '入站 action'
                                    }
                                    accentColor="var(--color-brand)"
                                    scaleMode={fitData ? 'fit' : 'zero'}
                                    showDots={showDots}
                                    className="h-full min-h-0"
                                />
                            )}
                        </div>
                    </div>
                </Panel>
            </div>
        </div>
    );
}

export default BotRuntimeMetricsPageNext;
