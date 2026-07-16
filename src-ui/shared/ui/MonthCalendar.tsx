// 主题化月历：区间高亮 / 今天 ring / 可选 min-max。
// 不依赖原生 date 控件。

import { useMemo } from 'react';
import { ChevronLeft, ChevronRight } from 'lucide-react';
import { cn } from '../utils/cn';

const WEEKDAY_LABELS = ['日', '一', '二', '三', '四', '五', '六'] as const;

export interface MonthCalendarProps {
    month: Date;
    onMonthChange: (next: Date) => void;
    /** 单选日（无区间时） */
    selectedDayMs?: number | null;
    /** 区间起（含） */
    rangeStartMs?: number | null;
    /** 区间止（含） */
    rangeEndMs?: number | null;
    onSelectDay: (dayStartMs: number) => void;
    minDayMs?: number;
    maxDayMs?: number;
    className?: string;
}

function startOfLocalDay(ms: number): number {
    const d = new Date(ms);
    d.setHours(0, 0, 0, 0);
    return d.getTime();
}

function getCalendarDays(month: Date): Date[] {
    const first = new Date(month.getFullYear(), month.getMonth(), 1);
    const gridStart = new Date(first);
    gridStart.setDate(first.getDate() - first.getDay());
    return Array.from({ length: 42 }, (_, i) => {
        const d = new Date(gridStart);
        d.setDate(gridStart.getDate() + i);
        return d;
    });
}

export function MonthCalendar({
    month,
    onMonthChange,
    selectedDayMs,
    rangeStartMs,
    rangeEndMs,
    onSelectDay,
    minDayMs,
    maxDayMs,
    className,
}: MonthCalendarProps) {
    const year = month.getFullYear();
    const mon = month.getMonth();
    const days = useMemo(
        () => getCalendarDays(new Date(year, mon, 1)),
        [year, mon],
    );

    const rangeStart =
        rangeStartMs != null ? startOfLocalDay(rangeStartMs) : null;
    const rangeEnd = rangeEndMs != null ? startOfLocalDay(rangeEndMs) : null;
    const selected =
        selectedDayMs != null ? startOfLocalDay(selectedDayMs) : null;
    const today = startOfLocalDay(Date.now());

    const title = new Intl.DateTimeFormat('zh-CN', {
        year: 'numeric',
        month: 'long',
    }).format(new Date(year, mon, 1));

    const canPrev =
        minDayMs == null ||
        new Date(year, mon, 0).getTime() >= minDayMs - 86400_000;
    const canNext =
        maxDayMs == null || new Date(year, mon + 1, 1).getTime() <= maxDayMs;

    return (
        <div
            className={cn(
                'flex h-full min-h-0 min-w-0 flex-col rounded-md bg-inset/40 p-2.5 ring-1 ring-border-subtle',
                className,
            )}
        >
            <div className="mb-1.5 flex shrink-0 items-center justify-between gap-1">
                <button
                    type="button"
                    disabled={!canPrev}
                    onClick={() => onMonthChange(new Date(year, mon - 1, 1))}
                    className="inline-flex h-7 w-7 items-center justify-center rounded-sm text-text-tertiary transition-colors hover:bg-elevated hover:text-text disabled:pointer-events-none disabled:opacity-35"
                    aria-label="上个月"
                >
                    <ChevronLeft aria-hidden size={14} />
                </button>
                <button
                    type="button"
                    className="rounded-sm px-1.5 text-[12px] font-semibold text-text transition-colors hover:text-brand"
                    onClick={() =>
                        onMonthChange(
                            new Date(
                                new Date().getFullYear(),
                                new Date().getMonth(),
                                1,
                            ),
                        )
                    }
                    title="回到本月"
                >
                    {title}
                </button>
                <button
                    type="button"
                    disabled={!canNext}
                    onClick={() => onMonthChange(new Date(year, mon + 1, 1))}
                    className="inline-flex h-7 w-7 items-center justify-center rounded-sm text-text-tertiary transition-colors hover:bg-elevated hover:text-text disabled:pointer-events-none disabled:opacity-35"
                    aria-label="下个月"
                >
                    <ChevronRight aria-hidden size={14} />
                </button>
            </div>

            <div className="mb-0.5 grid shrink-0 grid-cols-7 text-center text-[10px] font-medium text-text-tertiary">
                {WEEKDAY_LABELS.map((w) => (
                    <div key={w} className="py-0.5">
                        {w}
                    </div>
                ))}
            </div>

            <div className="grid min-h-0 flex-1 grid-cols-7 grid-rows-6 gap-px">
                {days.map((day) => {
                    const dayStart = startOfLocalDay(day.getTime());
                    const inMonth = day.getMonth() === mon;
                    const disabled =
                        (minDayMs != null && dayStart < minDayMs) ||
                        (maxDayMs != null && dayStart > maxDayMs);
                    const isStart = rangeStart != null && dayStart === rangeStart;
                    const isEnd = rangeEnd != null && dayStart === rangeEnd;
                    const isEndpoint = isStart || isEnd;
                    const inRange =
                        rangeStart != null &&
                        rangeEnd != null &&
                        dayStart >= rangeStart &&
                        dayStart <= rangeEnd;
                    const isSelected = selected != null && dayStart === selected;
                    const isToday = dayStart === today;

                    return (
                        <button
                            key={day.toISOString()}
                            type="button"
                            disabled={disabled}
                            onClick={() => onSelectDay(dayStart)}
                            className={cn(
                                'relative min-h-7 w-full rounded-sm text-[11px] font-medium tabular-nums transition-colors',
                                !inMonth && 'text-text-disabled/45',
                                inMonth &&
                                !inRange &&
                                !isSelected &&
                                'text-text-secondary hover:bg-elevated hover:text-text',
                                inRange &&
                                !isEndpoint &&
                                'bg-brand-soft/45 text-brand',
                                (isEndpoint || isSelected) &&
                                'bg-brand font-semibold text-white shadow-sm',
                                isToday &&
                                !isEndpoint &&
                                !isSelected &&
                                'ring-1 ring-brand/35',
                                disabled && 'pointer-events-none opacity-30',
                            )}
                        >
                            {day.getDate()}
                        </button>
                    );
                })}
            </div>
        </div>
    );
}
