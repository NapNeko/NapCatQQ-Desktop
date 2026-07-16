// 主题化时刻选择：弹出双列滚轮，中间固定主题色高亮带，不显示滚动条。
// 嵌在 Popover 内时用 modal={false}，避免抢焦点。

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Clock3 } from 'lucide-react';
import { cn } from '../utils/cn';
import { Popover, PopoverContent, PopoverTrigger } from './Popover';

export interface TimeValue {
    hours: number;
    minutes: number;
}

export interface TimePickerProps {
    hours: number;
    minutes: number;
    onChange: (next: TimeValue) => void;
    disabled?: boolean;
    /** 触发器形态：图标按钮 / 带文案的字段按钮 */
    variant?: 'icon' | 'field';
    className?: string;
    /** 分钟步进，默认 1 */
    minuteStep?: number;
    'aria-label'?: string;
}

const ITEM_H = 28;
const VISIBLE = 5;
const PAD_Y = Math.floor(VISIBLE / 2) * ITEM_H;

function pad2(n: number): string {
    return String(n).padStart(2, '0');
}

function clampHour(h: number): number {
    if (!Number.isFinite(h)) return 0;
    return Math.max(0, Math.min(23, Math.round(h)));
}

function clampMinute(m: number, step: number): number {
    if (!Number.isFinite(m)) return 0;
    const s = Math.max(1, step);
    const rounded = Math.round(m / s) * s;
    return Math.max(0, Math.min(59, rounded));
}

function nearestIndex(scrollTop: number, count: number): number {
    if (count <= 0) return 0;
    const raw = Math.round(scrollTop / ITEM_H);
    return Math.max(0, Math.min(count - 1, raw));
}

function WheelColumn({
    values,
    value,
    onPick,
    label,
    disabled,
}: {
    values: number[];
    value: number;
    onPick: (n: number) => void;
    label: string;
    disabled?: boolean;
}) {
    const listRef = useRef<HTMLDivElement | null>(null);
    const suppressScrollRef = useRef(false);
    const settleTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
    const valueIndex = Math.max(0, values.indexOf(value));

    const scrollToIndex = useCallback((index: number, behavior: ScrollBehavior = 'auto') => {
        const list = listRef.current;
        if (!list) return;
        suppressScrollRef.current = true;
        list.scrollTo({ top: index * ITEM_H, behavior });
        window.setTimeout(() => {
            suppressScrollRef.current = false;
        }, behavior === 'smooth' ? 180 : 0);
    }, []);

    useEffect(() => {
        scrollToIndex(valueIndex, 'auto');
    }, [valueIndex, values.length, scrollToIndex]);

    useEffect(() => {
        return () => {
            if (settleTimerRef.current) clearTimeout(settleTimerRef.current);
        };
    }, []);

    const commitFromScroll = useCallback(() => {
        const list = listRef.current;
        if (!list || disabled) return;
        const idx = nearestIndex(list.scrollTop, values.length);
        const next = values[idx];
        if (next === undefined) return;
        if (Math.abs(list.scrollTop - idx * ITEM_H) > 0.5) {
            scrollToIndex(idx, 'smooth');
        }
        if (next !== value) onPick(next);
    }, [disabled, onPick, scrollToIndex, value, values]);

    const onScroll = () => {
        if (suppressScrollRef.current || disabled) return;
        if (settleTimerRef.current) clearTimeout(settleTimerRef.current);
        settleTimerRef.current = setTimeout(commitFromScroll, 80);
    };

    return (
        // 外层裁切：即使 WebView 仍画出滚动条，也挤到可视区外
        <div className={cn('h-full w-[2.75rem] overflow-hidden', disabled && 'pointer-events-none')}>
            <div
                ref={listRef}
                role="listbox"
                aria-label={label}
                aria-disabled={disabled || undefined}
                tabIndex={disabled ? -1 : 0}
                onScroll={onScroll}
                onKeyDown={(e) => {
                    if (disabled) return;
                    if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
                        e.preventDefault();
                        const delta = e.key === 'ArrowUp' ? -1 : 1;
                        const nextIdx = Math.max(0, Math.min(values.length - 1, valueIndex + delta));
                        const next = values[nextIdx];
                        if (next === undefined) return;
                        scrollToIndex(nextIdx, 'smooth');
                        if (next !== value) onPick(next);
                    }
                }}
                className={cn(
                    // 全局 * 滚动条 hover 会显现；.scrollbar-hide 关掉 + 外层 overflow 裁切兜底
                    'scrollbar-hide h-full w-[calc(100%+1rem)] overflow-y-auto overscroll-contain pr-4',
                    'snap-y snap-mandatory',
                )}
                style={{ paddingTop: PAD_Y, paddingBottom: PAD_Y }}
            >
                {values.map((n, i) => {
                    const selected = n === value;
                    return (
                        <button
                            key={n}
                            type="button"
                            role="option"
                            aria-selected={selected}
                            tabIndex={-1}
                            disabled={disabled}
                            onClick={() => {
                                if (disabled) return;
                                scrollToIndex(i, 'smooth');
                                if (n !== value) onPick(n);
                            }}
                            className={cn(
                                'flex w-[2.75rem] snap-center items-center justify-center font-mono text-[12px] tabular-nums transition-colors',
                                selected
                                    ? 'font-semibold text-brand'
                                    : 'text-text-tertiary hover:text-text-secondary',
                            )}
                            style={{ height: ITEM_H }}
                        >
                            {pad2(n)}
                        </button>
                    );
                })}
            </div>
        </div>
    );
}

export function TimePicker({
    hours,
    minutes,
    onChange,
    disabled,
    variant = 'icon',
    className,
    minuteStep = 1,
    'aria-label': ariaLabel = '选择时刻',
}: TimePickerProps) {
    const [open, setOpen] = useState(false);
    const h = clampHour(hours);
    const m = clampMinute(minutes, minuteStep);
    const step = Math.max(1, minuteStep);

    const hourValues = useMemo(
        () => Array.from({ length: 24 }, (_, i) => i),
        [],
    );
    const minuteValues = useMemo(
        () =>
            Array.from({ length: Math.floor(60 / step) }, (_, i) => i * step),
        [step],
    );

    const trigger =
        variant === 'field' ? (
            <button
                type="button"
                disabled={disabled}
                aria-label={ariaLabel}
                className={cn(
                    'inline-flex h-8 items-center gap-1.5 rounded-sm border border-border-subtle bg-field px-2 font-mono text-[12px] tabular-nums text-text transition-colors',
                    'hover:border-border focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/40',
                    'disabled:cursor-not-allowed disabled:opacity-50',
                    className,
                )}
            >
                <Clock3 aria-hidden size={13} strokeWidth={2.1} className="text-text-tertiary" />
                <span>
                    {pad2(h)}:{pad2(m)}
                </span>
            </button>
        ) : (
            <button
                type="button"
                disabled={disabled}
                aria-label={ariaLabel}
                className={cn(
                    'inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-sm text-text-tertiary transition-colors',
                    'hover:bg-inset hover:text-brand focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/40',
                    'disabled:pointer-events-none disabled:opacity-50',
                    className,
                )}
            >
                <Clock3 aria-hidden size={13} strokeWidth={2.1} />
            </button>
        );

    return (
        <Popover open={open} onOpenChange={setOpen} modal={false}>
            <PopoverTrigger asChild>{trigger}</PopoverTrigger>
            <PopoverContent
                side="bottom"
                align="end"
                sideOffset={6}
                className="w-auto p-2"
                // 嵌在其它 Popover 内时：不要抢焦点、不要因外层 dismiss 逻辑抖动
                onOpenAutoFocus={(e) => e.preventDefault()}
                onCloseAutoFocus={(e) => e.preventDefault()}
            >
                <div
                    role="group"
                    aria-label={ariaLabel}
                    className="relative isolate select-none"
                    style={{ height: VISIBLE * ITEM_H }}
                >
                    {/* 中间主题色高亮带：固定指示当前选中行 */}
                    <div
                        aria-hidden
                        className="pointer-events-none absolute inset-x-0 top-1/2 z-0 h-7 -translate-y-1/2 rounded-md bg-brand/15 ring-1 ring-brand/35"
                    />
                    {/* 上下渐隐，突出中间选中 */}
                    <div
                        aria-hidden
                        className="pointer-events-none absolute inset-x-0 top-0 z-[1] h-7 rounded-t-md bg-gradient-to-b from-elevated to-transparent"
                    />
                    <div
                        aria-hidden
                        className="pointer-events-none absolute inset-x-0 bottom-0 z-[1] h-7 rounded-b-md bg-gradient-to-t from-elevated to-transparent"
                    />

                    <div className="relative z-[2] flex h-full items-stretch justify-center gap-1">
                        <WheelColumn
                            label="时"
                            values={hourValues}
                            value={h}
                            onPick={(nextH) => onChange({ hours: nextH, minutes: m })}
                        />
                        <span
                            aria-hidden
                            className="flex w-2 shrink-0 items-center justify-center font-mono text-sm font-semibold text-brand"
                        >
                            :
                        </span>
                        <WheelColumn
                            label="分"
                            values={minuteValues}
                            value={m}
                            onPick={(nextM) => onChange({ hours: h, minutes: nextM })}
                        />
                    </div>
                </div>
            </PopoverContent>
        </Popover>
    );
}

export function formatTimeValue(hours: number, minutes: number): string {
    return `${pad2(clampHour(hours))}:${pad2(clampMinute(minutes, 1))}`;
}

export function timeFromMs(ms: number): TimeValue {
    const d = new Date(ms);
    return { hours: d.getHours(), minutes: d.getMinutes() };
}

export function applyTimeToMs(baseMs: number, time: TimeValue): number {
    const d = new Date(baseMs);
    d.setHours(clampHour(time.hours), clampMinute(time.minutes, 1), 0, 0);
    return d.getTime();
}
