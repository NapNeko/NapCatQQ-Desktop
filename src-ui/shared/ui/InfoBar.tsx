// 顶层消息条原子件。对齐 legacy qfluentwidgets InfoBar 的语义：
//   - tone 决定颜色 + 图标（info / success / warning / danger）
//   - 标题 + 内容（content 可选；只放短句，长字符串自动折行 + 等宽 mono 化）
//   - 右上角 close 按钮 + 可选 autoDismissMs 自动消失
//   - slide-in 动画 + 退场轻淡出（进场是首要的，退场不重）
//
// 这一层只管展示，不管"何时该出现"。出现时机由上层 hook 推到
// InfoBarStack（通常监听 store 终态事件）。

import { forwardRef, useEffect, useRef, type HTMLAttributes, type ReactNode } from 'react';
import { AlertCircle, CheckCircle2, Info, X, AlertTriangle } from 'lucide-react';
import { cva, type VariantProps } from 'class-variance-authority';
import { cn } from '../utils/cn';

const infoBarVariants = cva(
    [
        // 基础布局
        'pointer-events-auto relative flex w-full items-start gap-3 overflow-hidden',
        'rounded-md border px-3.5 py-3 shadow-popover backdrop-blur-sm',
        // 进场动画：从右侧滑入 + 淡入
        'animate-[infobar-in_220ms_cubic-bezier(0.2,0.7,0.2,1)_both]',
    ],
    {
        variants: {
            tone: {
                info: 'border-info/25 bg-info-soft/85 text-text',
                success: 'border-success/25 bg-success-soft/85 text-text',
                warning: 'border-warning/30 bg-warning-soft/90 text-text',
                danger: 'border-danger/30 bg-danger-soft/90 text-text',
            },
        },
        defaultVariants: { tone: 'info' },
    },
);

const iconVariants = cva('mt-0.5 shrink-0', {
    variants: {
        tone: {
            info: 'text-info',
            success: 'text-success',
            warning: 'text-warning',
            danger: 'text-danger',
        },
    },
    defaultVariants: { tone: 'info' },
});

const titleColorVariants = cva('font-semibold leading-tight', {
    variants: {
        tone: {
            info: 'text-info',
            success: 'text-success',
            warning: 'text-warning',
            danger: 'text-danger',
        },
    },
    defaultVariants: { tone: 'info' },
});

function defaultIconFor(tone: NonNullable<VariantProps<typeof infoBarVariants>['tone']>) {
    switch (tone) {
        case 'success':
            return CheckCircle2;
        case 'warning':
            return AlertTriangle;
        case 'danger':
            return AlertCircle;
        case 'info':
        default:
            return Info;
    }
}

export interface InfoBarProps
    extends Omit<HTMLAttributes<HTMLDivElement>, 'title' | 'content'>,
        VariantProps<typeof infoBarVariants> {
    /** 顶部标题，简短一行，例如 "安装失败"。 */
    title: ReactNode;
    /** 详细文本。可空；为空时只渲染标题。 */
    content?: ReactNode;
    /** 自动消失毫秒数。0 / undefined 表示不自动消失（错误条默认行为）。 */
    autoDismissMs?: number;
    /** 用户点击 close 或自动消失时回调。父级通常拿这个把自身从 stack 移除。 */
    onDismiss?: () => void;
    /** 是否显示关闭按钮，默认 true。某些场景（如轻提示）可以关掉。 */
    closable?: boolean;
}

export const InfoBar = forwardRef<HTMLDivElement, InfoBarProps>(
    (
        {
            tone = 'info',
            title,
            content,
            autoDismissMs,
            onDismiss,
            closable = true,
            className,
            children,
            ...rest
        },
        ref,
    ) => {
        // autoDismiss 计时器。组件卸载或 props 变化时清。
        const onDismissRef = useRef(onDismiss);
        onDismissRef.current = onDismiss;
        useEffect(() => {
            if (!autoDismissMs || autoDismissMs <= 0) return;
            const id = setTimeout(() => onDismissRef.current?.(), autoDismissMs);
            return () => clearTimeout(id);
        }, [autoDismissMs]);

        const Icon = defaultIconFor(tone ?? 'info');

        return (
            <div
                ref={ref}
                role="alert"
                className={cn(infoBarVariants({ tone }), className)}
                {...rest}
            >
                <Icon size={16} strokeWidth={2.2} className={iconVariants({ tone })} />
                <div className="min-w-0 flex-1">
                    <div className={titleColorVariants({ tone })}>{title}</div>
                    {content && (
                        <div className="mt-1 break-words text-[12.5px] leading-relaxed text-text-secondary">
                            {content}
                        </div>
                    )}
                    {children}
                </div>
                {closable && (
                    <button
                        type="button"
                        aria-label="关闭"
                        onClick={() => onDismiss?.()}
                        className={cn(
                            '-mr-1 -mt-1 inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-sm',
                            'text-text-tertiary transition-colors',
                            'hover:bg-inset hover:text-text',
                            'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-brand',
                        )}
                    >
                        <X size={13} strokeWidth={2.2} />
                    </button>
                )}
            </div>
        );
    },
);
InfoBar.displayName = 'InfoBar';
