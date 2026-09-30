// 左栏三个面板共用的小件：带提示的图标按钮、紧凑搜索框、分段筛选、空 / 错状态、确认框、骨架行，
// 以及「按 / 聚焦搜索框」。

import { forwardRef, useEffect, useRef, useState, type ReactNode, type RefObject } from 'react';
import { Search, X } from 'lucide-react';
import type { LucideIcon } from 'lucide-react';
import { cn } from '../../../shared/utils/cn';
import {
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    Spinner,
    Tooltip,
    TooltipContent,
    TooltipTrigger,
} from '../../../shared/ui';
import { Shimmer } from '../../../shared/ui/motion';

// ---------------------------------------------------------------------------
// 图标按钮
// ---------------------------------------------------------------------------

export interface IconActionProps {
    icon: LucideIcon;
    /** 同时是 aria-label 和提示文字 */
    label: string;
    onClick: (e: React.MouseEvent<HTMLButtonElement>) => void;
    tone?: 'default' | 'danger' | 'brand';
    size?: 'sm' | 'md';
    /** 给了就禁用，并把原因放进提示里（禁用的按钮收不到指针事件，提示挂在外层 span 上） */
    disabledReason?: string | null;
    busy?: boolean;
    className?: string;
    tooltipSide?: 'top' | 'bottom' | 'left' | 'right';
    /**
     * false：不进 Tab 顺序。列表行上悬停才出现的按钮用它——键盘走行上的快捷键（回车、F2、Delete…），
     * 否则每行三四个按钮会把 Tab 顺序撑得很长
     */
    focusable?: boolean;
}

export function IconAction({
    icon: Icon,
    label,
    onClick,
    tone = 'default',
    size = 'sm',
    disabledReason = null,
    busy = false,
    className,
    tooltipSide = 'top',
    focusable = true,
}: IconActionProps) {
    const disabled = !!disabledReason || busy;
    const button = (
        <button
            type="button"
            aria-label={label}
            disabled={disabled}
            tabIndex={focusable ? undefined : -1}
            onClick={(e) => {
                // 行本身也能点：按钮上的点击别再冒上去打开请求
                e.stopPropagation();
                onClick(e);
            }}
            onPointerDown={(e) => e.stopPropagation()}
            className={cn(
                'inline-flex shrink-0 items-center justify-center rounded-sm transition-colors',
                'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand',
                'disabled:cursor-not-allowed disabled:opacity-40',
                size === 'sm' ? 'h-6 w-6' : 'h-7 w-7',
                tone === 'danger'
                    ? 'text-text-tertiary hover:bg-danger-soft hover:text-danger'
                    : tone === 'brand'
                      ? 'text-text-tertiary hover:bg-brand-soft hover:text-brand'
                      : 'text-text-tertiary hover:bg-inset hover:text-text',
                className,
            )}
        >
            {busy ? <Spinner size="xs" label={label} /> : <Icon size={size === 'sm' ? 13 : 14} strokeWidth={2} aria-hidden />}
        </button>
    );
    return (
        <Tooltip>
            <TooltipTrigger asChild>{disabled ? <span className="inline-flex">{button}</span> : button}</TooltipTrigger>
            <TooltipContent side={tooltipSide}>{disabledReason ? `${label}：${disabledReason}` : label}</TooltipContent>
        </Tooltip>
    );
}

// ---------------------------------------------------------------------------
// 搜索框
// ---------------------------------------------------------------------------

export interface PanelSearchProps {
    value: string;
    onChange: (v: string) => void;
    placeholder: string;
    ariaLabel: string;
    /** 右侧的小字，比如结果数 */
    trailing?: ReactNode;
    onKeyDown?: (e: React.KeyboardEvent<HTMLInputElement>) => void;
    onFocus?: () => void;
    onBlur?: () => void;
    /** 组合框用：列表 id 和当前高亮行 id */
    controls?: string;
    activeDescendant?: string;
}

export const PanelSearch = forwardRef<HTMLInputElement, PanelSearchProps>(function PanelSearch(
    { value, onChange, placeholder, ariaLabel, trailing, onKeyDown, onFocus, onBlur, controls, activeDescendant },
    ref,
) {
    return (
        <label className="relative flex min-w-0 flex-1 items-center">
            <Search size={13} aria-hidden className="pointer-events-none absolute left-2.5 text-text-tertiary" />
            <input
                ref={ref}
                type="text"
                value={value}
                spellCheck={false}
                autoComplete="off"
                onChange={(e) => onChange(e.target.value)}
                onKeyDown={(e) => {
                    onKeyDown?.(e);
                    if (e.defaultPrevented) return;
                    // 有字时 Esc 先清空；空了再按才交给别处（比如中栏的取消）
                    if (e.key === 'Escape' && value !== '') {
                        e.preventDefault();
                        onChange('');
                    }
                }}
                onFocus={onFocus}
                onBlur={onBlur}
                placeholder={placeholder}
                aria-label={ariaLabel}
                role={controls ? 'combobox' : undefined}
                aria-expanded={controls ? true : undefined}
                aria-controls={controls}
                aria-activedescendant={activeDescendant}
                aria-autocomplete={controls ? 'list' : undefined}
                className={cn(
                    'h-7 w-full rounded-sm border border-border-subtle bg-field pl-7 text-[12px] text-text outline-none transition-colors',
                    'placeholder:text-text-tertiary focus:border-brand focus:ring-2 focus:ring-inset focus:ring-brand',
                    value ? 'pr-14' : 'pr-2',
                )}
            />
            {value && (
                <span className="absolute right-1 flex items-center gap-1">
                    {trailing !== undefined && (
                        <span className="text-[10px] tabular-nums text-text-tertiary">{trailing}</span>
                    )}
                    <button
                        type="button"
                        aria-label="清空搜索"
                        title="清空搜索"
                        onClick={() => onChange('')}
                        className="inline-flex h-5 w-5 items-center justify-center rounded-xs text-text-tertiary transition-colors hover:bg-inset hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
                    >
                        <X size={12} aria-hidden />
                    </button>
                </span>
            )}
        </label>
    );
});

/**
 * 页面上没在打字时按 `/` 聚焦搜索框。输入框、编辑器、对话框、终端里的 `/` 照常输入；
 * 页面被终端盖住（看不见）时不抢。
 */
export function useSlashFocus(inputRef: RefObject<HTMLInputElement | null>, scopeRef: RefObject<HTMLElement | null>): void {
    useEffect(() => {
        const onKeyDown = (e: KeyboardEvent) => {
            if (e.key !== '/' || e.ctrlKey || e.metaKey || e.altKey || e.defaultPrevented || e.isComposing) return;
            const t = e.target;
            if (
                t instanceof HTMLElement &&
                (t.isContentEditable ||
                    t.closest('input, textarea, select, [contenteditable="true"], .cm-editor, [role="dialog"], [role="alertdialog"], .xterm'))
            ) {
                return;
            }
            const scope = scopeRef.current;
            if (!scope || !scope.isConnected) return;
            if (typeof scope.checkVisibility === 'function' && !scope.checkVisibility()) return;
            const input = inputRef.current;
            if (!input) return;
            e.preventDefault();
            input.focus();
            input.select();
        };
        window.addEventListener('keydown', onKeyDown);
        return () => window.removeEventListener('keydown', onKeyDown);
    }, [inputRef, scopeRef]);
}

// ---------------------------------------------------------------------------
// 分段筛选（和任务队列页的筛选同一套样子）
// ---------------------------------------------------------------------------

export function Segmented<T extends string>({
    value,
    options,
    onChange,
    ariaLabel,
}: {
    value: T;
    options: ReadonlyArray<{ id: T; label: string }>;
    onChange: (v: T) => void;
    ariaLabel: string;
}) {
    return (
        <div role="radiogroup" aria-label={ariaLabel} className="inline-flex shrink-0 items-center gap-0.5 rounded-md bg-inset p-0.5">
            {options.map((o) => {
                const on = o.id === value;
                return (
                    <button
                        key={o.id}
                        type="button"
                        role="radio"
                        aria-checked={on}
                        onClick={() => onChange(o.id)}
                        className={cn(
                            'inline-flex items-center whitespace-nowrap rounded-sm px-2 py-0.5 text-[11px] font-medium transition-colors',
                            'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-1 focus-visible:ring-offset-canvas',
                            on
                                ? 'bg-elevated text-text shadow-sm ring-1 ring-border-subtle'
                                : 'text-text-tertiary hover:bg-elevated/35 hover:text-text',
                        )}
                    >
                        {o.label}
                    </button>
                );
            })}
        </div>
    );
}

// ---------------------------------------------------------------------------
// 空状态 / 出错 / 加载
// ---------------------------------------------------------------------------

export function PanelMessage({
    icon: Icon,
    title,
    hint,
    tone = 'neutral',
    action,
}: {
    icon: LucideIcon;
    title: string;
    hint?: ReactNode;
    tone?: 'neutral' | 'danger';
    action?: ReactNode;
}) {
    return (
        <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 overflow-y-auto px-5 py-8 text-center">
            <span
                className={cn(
                    'inline-flex h-9 w-9 items-center justify-center rounded-md',
                    tone === 'danger' ? 'bg-danger-soft text-danger' : 'bg-inset text-text-tertiary',
                )}
            >
                <Icon size={17} strokeWidth={1.9} aria-hidden />
            </span>
            <p className="font-display text-[13px] font-semibold text-text-secondary">{title}</p>
            {hint && <p className="max-w-[26rem] text-2xs leading-relaxed text-text-tertiary">{hint}</p>}
            {action}
        </div>
    );
}

/** 列表还没到时的骨架：和真实行一样高，内容到了不跳 */
export function SkeletonRows({ rows = 8, rowHeight = 30 }: { rows?: number; rowHeight?: number }) {
    return (
        <div aria-hidden className="flex flex-col gap-0 px-2 py-1.5">
            {Array.from({ length: rows }, (_, i) => (
                <div key={i} className="flex items-center gap-2 px-1.5" style={{ height: rowHeight }}>
                    <Shimmer height={7} className="w-[7px] shrink-0 !rounded-full" />
                    <Shimmer height={10} className={i % 3 === 0 ? 'w-2/5' : i % 3 === 1 ? 'w-3/5' : 'w-1/2'} />
                </div>
            ))}
        </div>
    );
}

// ---------------------------------------------------------------------------
// 确认框
// ---------------------------------------------------------------------------

export interface ConfirmRequest {
    title: string;
    description: ReactNode;
    confirmLabel: string;
    tone?: 'danger' | 'primary';
    onConfirm: () => void;
    /**
     * 关掉后焦点落到哪。默认回到打开前的元素；从键盘删掉一行时那一行已经没了，
     * 由调用方指给相邻的行，焦点不至于掉回页面开头
     */
    restoreFocus?: () => HTMLElement | null | undefined;
}

/**
 * 删除、清空、危险调用前的确认。`request` 为 null 时关着；关的动画期间保留上一次的内容，
 * 标题不会在收起时变空。
 */
export function ConfirmDialog({ request, onClose }: { request: ConfirmRequest | null; onClose: () => void }) {
    const [shown, setShown] = useState<ConfirmRequest | null>(request);
    const confirmRef = useRef<HTMLButtonElement>(null);
    useEffect(() => {
        if (request) setShown(request);
    }, [request]);
    const content = request ?? shown;
    // 从没打开过就不挂：Dialog 的门户是常驻挂载的，每个面板都挂一个不划算
    if (!content) return null;
    return (
        <Dialog open={request !== null} onOpenChange={(o) => !o && onClose()}>
            <DialogContent
                size="sm"
                onOpenAutoFocus={(e) => {
                    // 焦点落在确认按钮上：回车即确认，Esc 取消
                    e.preventDefault();
                    confirmRef.current?.focus();
                }}
                onCloseAutoFocus={(e) => {
                    const target = content.restoreFocus?.();
                    if (!target) return;
                    e.preventDefault();
                    target.focus();
                }}
            >
                {content && (
                    <>
                        <DialogHeader>
                            <DialogTitle>{content.title}</DialogTitle>
                            <DialogDescription>{content.description}</DialogDescription>
                        </DialogHeader>
                        <DialogFooter>
                            <Button variant="ghost" size="sm" onClick={onClose}>
                                取消
                            </Button>
                            <Button
                                ref={confirmRef}
                                variant={content.tone === 'primary' ? 'primary' : 'danger'}
                                size="sm"
                                onClick={() => {
                                    onClose();
                                    content.onConfirm();
                                }}
                            >
                                {content.confirmLabel}
                            </Button>
                        </DialogFooter>
                    </>
                )}
            </DialogContent>
        </Dialog>
    );
}
