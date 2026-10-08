// 左栏三个面板（接口 / 收藏 / 历史）的标签组：图标 + 文字，窄到放不下时只留图标。
import { useRef, type KeyboardEvent as ReactKeyboardEvent } from 'react';
import { cn } from '../../../shared/utils/cn';
import { Tooltip, TooltipContent, TooltipTrigger } from '../../../shared/ui';
import { LEFT_PANELS, type DebugLeftPanel } from '../leftPanels';
import { nextIndex } from '../../../core/domain/debug/tabCycling';

/** 标签组的可访问名；收起左栏后回焦的查询选择器也用它 */
export const LEFT_TABS_LABEL = '左栏面板';

export function LeftPanelTabs({
    active,
    onChange,
}: {
    active: DebugLeftPanel;
    onChange: (p: DebugLeftPanel) => void;
}) {
    const refs = useRef<Partial<Record<DebugLeftPanel, HTMLButtonElement | null>>>({});
    const onKeyDown = (e: ReactKeyboardEvent<HTMLDivElement>) => {
        if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
        e.preventDefault();
        const idx = LEFT_PANELS.findIndex((p) => p.id === active);
        const next =
            LEFT_PANELS[nextIndex(idx, LEFT_PANELS.length, e.key === 'ArrowRight' ? 1 : -1)];
        onChange(next.id);
        refs.current[next.id]?.focus();
    };
    // 左栏拖到最窄（200px）时三个「图标 + 字」放不下，字会被截成「接.」：这时只留图标，字留给读屏，
    // 悬停提示照样写全名。提示在宽的时候也有，写的是比标签更完整的说法
    return (
        <div
            role="tablist"
            aria-label={LEFT_TABS_LABEL}
            onKeyDown={onKeyDown}
            className="@container/lefttabs flex min-w-0 flex-1 items-center gap-0.5 rounded-md bg-inset p-0.5"
        >
            {LEFT_PANELS.map((p) => {
                const on = p.id === active;
                const Icon = p.icon;
                return (
                    <Tooltip key={p.id}>
                        <TooltipTrigger asChild>
                            <button
                                ref={(node) => {
                                    refs.current[p.id] = node;
                                }}
                                type="button"
                                role="tab"
                                aria-selected={on}
                                tabIndex={on ? 0 : -1}
                                onClick={() => onChange(p.id)}
                                className={cn(
                                    'inline-flex min-w-0 flex-1 items-center justify-center gap-1 rounded-sm px-1.5 py-1 text-[12px] font-medium transition-colors',
                                    'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-1 focus-visible:ring-offset-canvas',
                                    on
                                        ? 'bg-elevated text-text shadow-sm ring-1 ring-border-subtle'
                                        : 'text-text-tertiary hover:bg-elevated/35 hover:text-text',
                                )}
                            >
                                <Icon
                                    size={12}
                                    strokeWidth={2.2}
                                    aria-hidden
                                    className="shrink-0"
                                />
                                <span className="truncate @max-[168px]/lefttabs:sr-only">
                                    {p.label}
                                </span>
                            </button>
                        </TooltipTrigger>
                        <TooltipContent side="bottom">{p.hint}</TooltipContent>
                    </Tooltip>
                );
            })}
        </div>
    );
}
