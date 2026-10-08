// 趋势图时间范围面板：预设快捷 + 自定义起止（月历选日、TimePicker 选时刻）
// 从指标页外提；TimePicker 也是 Popover portal，outside 事件拦截逻辑不能丢。

import { useEffect, useState } from 'react';
import { CalendarRange, ChevronDown } from 'lucide-react';
import {
    applyTimeToMs,
    Button,
    Checkbox,
    formatTimeValue,
    MonthCalendar,
    Popover,
    PopoverContent,
    PopoverTrigger,
    timeFromMs,
    TimePicker,
} from '../../../shared/ui';
import {
    combineLocalDateAndTime,
    formatHistoryWindowLabel,
    formatLocalDateLabel,
    isMetricsHistoryRangeAvailable,
    METRICS_HISTORY_RANGE_OPTIONS,
    resolveHistoryWindowBounds,
    startOfLocalDay,
    type MetricsHistoryRange,
    type MetricsHistoryWindow,
} from '../../../core/domain/bot/runtime-metrics-settings';
import { cn } from '../../../shared/utils/cn';

export function TrendRangePanel({
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
    const [monthCursor, setMonthCursor] = useState(() => new Date(startOfLocalDay(bounds.fromMs)));
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
    const presetActive = window.mode === 'preset' ? window.range : null;

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
                        aria-label={field === 'from' ? '选择开始时刻' : '选择结束时刻'}
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
                        const ok = isMetricsHistoryRangeAvailable(opt.id, retentionDays);
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
                                    ok ? opt.label : `保留 ${retentionDays} 天，无法选 ${opt.label}`
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

                        {error ? <p className="text-[11px] text-danger">{error}</p> : null}

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
                            <Button type="button" size="sm" className="flex-1" onClick={apply}>
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
