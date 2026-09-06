// 「每月几号」选择：字段按钮弹出 7 列格子（1–31），单选即关。
// 不是日历：没有年月、没有星期，纯粹选一个日期数字。

import { useState } from 'react';
import { CalendarDays } from 'lucide-react';
import { cn } from '../utils/cn';
import { Popover, PopoverContent, PopoverTrigger } from './Popover';

export interface DayOfMonthPickerProps {
    /** 1–31 */
    day: number;
    onChange: (day: number) => void;
    disabled?: boolean;
    className?: string;
    'aria-label'?: string;
}

const DAYS = Array.from({ length: 31 }, (_, i) => i + 1);

function clampDay(d: number): number {
    if (!Number.isFinite(d)) return 1;
    return Math.max(1, Math.min(31, Math.round(d)));
}

export function DayOfMonthPicker({
    day,
    onChange,
    disabled,
    className,
    'aria-label': ariaLabel = '选择日期',
}: DayOfMonthPickerProps) {
    const [open, setOpen] = useState(false);
    const current = clampDay(day);

    return (
        <Popover open={open} onOpenChange={setOpen} modal={false}>
            <PopoverTrigger asChild>
                <button
                    type="button"
                    disabled={disabled}
                    aria-label={ariaLabel}
                    className={cn(
                        'inline-flex h-8 items-center gap-1.5 rounded-sm border border-border-subtle bg-field px-2 text-[12px] tabular-nums text-text transition-colors',
                        'hover:border-border focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/40',
                        'data-[state=open]:border-brand data-[state=open]:ring-2 data-[state=open]:ring-brand data-[state=open]:ring-inset',
                        'disabled:cursor-not-allowed disabled:opacity-50',
                        className,
                    )}
                >
                    <CalendarDays aria-hidden size={13} strokeWidth={2.1} className="text-text-tertiary" />
                    <span>{current} 日</span>
                </button>
            </PopoverTrigger>
            <PopoverContent
                side="bottom"
                align="start"
                sideOffset={6}
                className="w-auto p-2"
                onOpenAutoFocus={(e) => e.preventDefault()}
                onCloseAutoFocus={(e) => e.preventDefault()}
            >
                <div role="listbox" aria-label={ariaLabel} className="grid grid-cols-7 gap-px">
                    {DAYS.map((d) => {
                        const selected = d === current;
                        return (
                            <button
                                key={d}
                                type="button"
                                role="option"
                                aria-selected={selected}
                                onClick={() => {
                                    if (d !== current) onChange(d);
                                    setOpen(false);
                                }}
                                className={cn(
                                    'h-7 w-8 rounded-sm text-[11px] font-medium tabular-nums transition-colors',
                                    selected
                                        ? 'bg-brand font-semibold text-white shadow-sm'
                                        : 'text-text-secondary hover:bg-inset hover:text-text',
                                    // 短月没有的日期用弱化色提醒，仍可选
                                    !selected && d > 28 && 'text-text-tertiary',
                                )}
                            >
                                {d}
                            </button>
                        );
                    })}
                </div>
            </PopoverContent>
        </Popover>
    );
}
