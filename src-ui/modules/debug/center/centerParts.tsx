// 中栏里反复出现的小零件：分段切换、带提示的图标按钮、输入框样式、复制到剪贴板。

import {
    forwardRef,
    useRef,
    type ButtonHTMLAttributes,
    type KeyboardEvent as ReactKeyboardEvent,
    type ReactNode,
} from 'react';
import type { LucideIcon } from 'lucide-react';
import { cn } from '../../../shared/utils/cn';
import { Tooltip, TooltipContent, TooltipTrigger } from '../../../shared/ui';
import { ActionMotionIcon } from '../../../shared/ui/motion';
import { pushInfoBar } from '../../../hooks/ui/globalInfoBarStore';
import { pushErrorBar } from '../../../hooks/ui/pushErrorBar';

/** 参数表单里输入框的样子：和 shared/ui 的 TextField / Select 同一套 token 与高度，表单里上下对得齐 */
export const FIELD_INPUT_CLASS = cn(
    'block h-[38px] w-full min-w-0 rounded-sm border bg-field px-3 text-sm text-text outline-none transition-colors duration-150',
    'placeholder:text-text-tertiary',
    'disabled:cursor-not-allowed disabled:bg-inset disabled:text-text-disabled',
    'focus:ring-2 focus:ring-inset',
);

export function fieldBorder(invalid: boolean): string {
    return invalid
        ? 'border-danger focus:border-danger focus:ring-danger'
        : 'border-border-subtle focus:border-brand focus:ring-brand';
}

// ---------------------------------------------------------------------------
// 分段切换（和左栏面板切换、任务队列筛选同一个样子）
// ---------------------------------------------------------------------------

export interface SegmentOption<V extends string> {
    value: V;
    label: ReactNode;
    disabled?: boolean;
    /** 禁用时说明为什么 */
    title?: string;
}

export function Segmented<V extends string>({
    options,
    value,
    onChange,
    label,
    size = 'sm',
    className,
}: {
    options: ReadonlyArray<SegmentOption<V>>;
    value: V;
    onChange: (v: V) => void;
    /** 给屏幕阅读器的组名 */
    label: string;
    size?: 'xs' | 'sm';
    className?: string;
}) {
    const refs = useRef<Array<HTMLButtonElement | null>>([]);
    const onKeyDown = (e: ReactKeyboardEvent<HTMLDivElement>) => {
        if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
        const enabled = options.map((o, i) => (o.disabled ? -1 : i)).filter((i) => i >= 0);
        const at = enabled.indexOf(options.findIndex((o) => o.value === value));
        const next =
            enabled[(at + (e.key === 'ArrowRight' ? 1 : -1) + enabled.length) % enabled.length];
        if (next === undefined) return;
        e.preventDefault();
        onChange(options[next]!.value);
        refs.current[next]?.focus();
    };
    return (
        <div
            role="radiogroup"
            aria-label={label}
            onKeyDown={onKeyDown}
            className={cn(
                'inline-flex shrink-0 items-center gap-0.5 rounded-md bg-inset p-0.5',
                className,
            )}
        >
            {options.map((o, i) => {
                const on = o.value === value;
                return (
                    <button
                        key={o.value}
                        ref={(node) => {
                            refs.current[i] = node;
                        }}
                        type="button"
                        role="radio"
                        aria-checked={on}
                        tabIndex={on ? 0 : -1}
                        disabled={o.disabled}
                        title={o.title}
                        onClick={() => onChange(o.value)}
                        className={cn(
                            'inline-flex items-center justify-center gap-1 rounded-sm font-medium transition-colors',
                            size === 'xs' ? 'h-5 px-1.5 text-[11px]' : 'h-6 px-2 text-[12px]',
                            'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-1 focus-visible:ring-offset-canvas',
                            'disabled:cursor-not-allowed disabled:opacity-45',
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
// 图标按钮：aria-label + 提示气泡
// ---------------------------------------------------------------------------

export interface IconTipProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'children'> {
    icon: LucideIcon;
    /** 同时用作 aria-label 和提示 */
    label: string;
    /** 提示里多一行说明（比如快捷键），不进 aria-label */
    hint?: string;
    size?: 'sm' | 'md';
    active?: boolean;
    tone?: 'default' | 'danger';
    side?: 'top' | 'bottom' | 'left' | 'right';
    children?: ReactNode;
}

export const IconTip = forwardRef<HTMLButtonElement, IconTipProps>(function IconTip(
    {
        icon,
        label,
        hint,
        size = 'md',
        active,
        tone = 'default',
        side = 'bottom',
        className,
        children,
        ...rest
    },
    ref,
) {
    return (
        <Tooltip>
            <TooltipTrigger asChild>
                <button
                    ref={ref}
                    type="button"
                    aria-label={label}
                    className={cn(
                        'inline-flex shrink-0 items-center justify-center gap-1 rounded-sm transition-colors',
                        size === 'sm' ? 'h-6 min-w-6 px-1' : 'h-7 min-w-7 px-1.5',
                        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-1 focus-visible:ring-offset-canvas',
                        'disabled:cursor-not-allowed disabled:opacity-45 disabled:hover:bg-transparent',
                        active
                            ? 'bg-brand-soft text-brand'
                            : tone === 'danger'
                              ? 'text-text-tertiary hover:bg-danger-soft hover:text-danger'
                              : 'text-text-tertiary hover:bg-inset hover:text-text',
                        className,
                    )}
                    {...rest}
                >
                    <ActionMotionIcon icon={icon} size={size === 'sm' ? 13 : 14} strokeWidth={2} />
                    {children}
                </button>
            </TooltipTrigger>
            <TooltipContent side={side}>
                {label}
                {hint && <span className="ml-1.5 opacity-70">{hint}</span>}
            </TooltipContent>
        </Tooltip>
    );
});

// ---------------------------------------------------------------------------
// 复制
// ---------------------------------------------------------------------------

/** 复制并弹一条轻提示；webview 拿不到剪贴板时弹错误条，不静默失败 */
export async function copyWithToast(text: string, title: string): Promise<void> {
    try {
        await navigator.clipboard.writeText(text);
        pushInfoBar({ key: 'debug-copy', tone: 'info', title, autoDismissMs: 2000 });
    } catch (err) {
        pushErrorBar({
            key: 'debug-copy',
            title: '复制失败',
            raw: err instanceof Error ? err.message : String(err),
        });
    }
}

/** 键盘提示小方块 */
export function Kbd({ children, className }: { children: ReactNode; className?: string }) {
    return (
        <kbd
            className={cn(
                'rounded-xs border border-current/25 px-1 py-px font-mono text-[10px] font-medium leading-none opacity-80',
                className,
            )}
        >
            {children}
        </kbd>
    );
}
