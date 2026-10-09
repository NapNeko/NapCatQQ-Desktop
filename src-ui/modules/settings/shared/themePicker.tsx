// 主题选择器：FieldRow 里的紧凑触发按钮 + 按族分组的主题卡片弹层。

import { useCallback, useEffect, useRef, useState } from 'react';
import { Popover, PopoverTrigger, PopoverContent } from '../../../shared/ui';
import { useMotion } from '../../../hooks/preferences/useMotion';
import { ChevronDown } from 'lucide-react';
import type { ThemeMode } from '../../../core/domain/settings/preferencesStore';
import { THEME_GROUPS, findThemePreview } from '../../../core/design/themes/registry';

// 主题多，弹层按 Radix 算出的可用高度收，窗口矮时在里面滚而不是伸出窗外；28px 留给弹层内边距。
const PICKER_SCROLL_STYLE = {
    maxHeight: 'min(540px, calc(var(--radix-popover-content-available-height, 540px) - 28px))',
};

/**
 * 主题选择器弹窗组件。
 * FieldRow 中展示紧凑触发按钮（色块 + 当前主题全名），点击弹出 Popover，
 * 弹窗内按族分组展示主题卡片。外层 4 列网格，每组占自己卡片数那么多列，
 * 3 个的族和 1 个的族能拼在同一行；组内卡片宽度和外层列宽一致。
 * 主题数据全在 core/design/themes，这里不列主题。
 */
export function ThemePicker({
    value,
    onChange,
}: {
    value: ThemeMode;
    onChange: (next: ThemeMode) => void;
}) {
    const [open, setOpen] = useState(false);
    const current = findThemePreview(value);
    const m = useMotion();

    // 卡片按钮 ref 映射，给 bindHover/bindPress 挂事件监听
    const cardRefs = useRef<Map<string, HTMLButtonElement>>(new Map());
    const setCardRef = useCallback(
        (val: string) => (el: HTMLButtonElement | null) => {
            if (el) cardRefs.current.set(val, el);
            else cardRefs.current.delete(val);
        },
        [],
    );

    // 卡片 hover/press 交互
    useEffect(() => {
        const cleanups: Array<() => void> = [];
        cardRefs.current.forEach((el) => {
            cleanups.push(m.bindHover(el));
            cleanups.push(m.bindPress(el));
        });
        return () => cleanups.forEach((fn) => fn());
    }, [m, open]);

    return (
        <Popover open={open} onOpenChange={setOpen}>
            {/* 触发按钮 */}
            <PopoverTrigger asChild>
                <button
                    type="button"
                    className="flex h-7 items-center gap-2 rounded-md bg-inset px-2.5 text-[12px] font-medium text-text transition-colors hover:bg-muted/50"
                >
                    {current && (
                        <span
                            className="h-3.5 w-3.5 shrink-0 rounded-sm"
                            style={{
                                background: current.brand,
                                boxShadow: 'inset 0 0 0 0.5px rgba(128,128,128,0.15)',
                            }}
                        />
                    )}
                    <span>{current?.fullLabel ?? value}</span>
                    <ChevronDown className="h-3 w-3 text-text-tertiary" />
                </button>
            </PopoverTrigger>

            {/* 弹窗内容 — 用户通过点击外部 / Escape / 再点触发器关闭 */}
            <PopoverContent side="bottom" align="start" sideOffset={6}>
                <div
                    className="-mr-1.5 grid grid-cols-[repeat(4,84px)] gap-x-1.5 gap-y-3 overflow-y-auto pr-1.5"
                    style={PICKER_SCROLL_STYLE}
                >
                    {THEME_GROUPS.map((group) => (
                        <div
                            key={group.label}
                            className="min-w-0 space-y-1.5"
                            style={{ gridColumn: `span ${Math.min(group.items.length, 4)}` }}
                        >
                            {/* 分组标签 */}
                            <span className="block truncate text-[11px] font-medium tracking-wide text-text-tertiary">
                                {group.label}
                            </span>
                            <div
                                className="grid gap-1.5"
                                style={{
                                    gridTemplateColumns: `repeat(${Math.min(group.items.length, 4)}, minmax(0, 1fr))`,
                                }}
                            >
                                {group.items.map((item) => {
                                    const selected = value === item.value;
                                    return (
                                        <button
                                            key={item.value}
                                            ref={setCardRef(item.value)}
                                            type="button"
                                            title={item.fullLabel}
                                            onClick={() => onChange(item.value)}
                                            className={
                                                'relative flex flex-col items-stretch gap-1 rounded-md p-1 transition-colors ' +
                                                (selected ? 'bg-surface' : 'hover:bg-muted/40')
                                            }
                                            style={
                                                selected
                                                    ? {
                                                          boxShadow: `inset 0 0 0 1px ${item.brand}44`,
                                                      }
                                                    : undefined
                                            }
                                        >
                                            {/* 缩略窗口预览 */}
                                            <div
                                                className="relative h-9 w-full overflow-hidden rounded-[3px]"
                                                style={{
                                                    background: item.canvas,
                                                    boxShadow:
                                                        'inset 0 0 0 0.5px rgba(128,128,128,0.1)',
                                                }}
                                            >
                                                <div
                                                    className="absolute inset-y-0 left-0 w-[30%]"
                                                    style={{ background: item.sidebar }}
                                                />
                                                <div className="absolute inset-y-0 right-0 left-[30%] flex flex-col justify-center gap-[3px] px-1.5">
                                                    <div
                                                        className="h-[2.5px] w-[60%] rounded-full"
                                                        style={{
                                                            background: item.text,
                                                            opacity: 0.5,
                                                        }}
                                                    />
                                                    <div
                                                        className="h-[2.5px] w-[40%] rounded-full"
                                                        style={{
                                                            background: item.subtext,
                                                            opacity: 0.4,
                                                        }}
                                                    />
                                                    <div
                                                        className="mt-[1px] h-[4px] w-[32%] rounded-full"
                                                        style={{ background: item.brand }}
                                                    />
                                                </div>
                                                <div
                                                    className="absolute right-1 top-1 h-[4px] w-[4px] rounded-full"
                                                    style={{ background: item.accent }}
                                                />
                                            </div>

                                            {/* 标签 — 字重固定避免选中时 font-semibold 撑宽 grid */}
                                            <span
                                                className={
                                                    'truncate text-center text-[11px] font-semibold leading-tight ' +
                                                    (selected ? 'text-text' : 'text-text-tertiary')
                                                }
                                            >
                                                {item.label}
                                            </span>
                                        </button>
                                    );
                                })}
                            </div>
                        </div>
                    ))}
                </div>
            </PopoverContent>
        </Popover>
    );
}
