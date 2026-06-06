// 顶层消息条原子件。GSAP 版。
//   - tone 决定颜色 + 图标(info / success / warning / danger)
//   - 标题 + 内容(content 可选)
//   - 右上角 close 按钮 + 可选 autoDismissMs 自动消失
//   - 进退场动画走 GSAP,由 InfoBarStack 通过 GsapPresence 管 mount/unmount
//
// 这一层只管展示,不管"何时该出现"。出现时机由上层 hook 推到
// InfoBarStack(通常监听 store 终态事件)。
//
// 注意:本组件 forwardRef 把 root div 暴露给外部,GsapPresence 才能拿到节点。

import { forwardRef, useEffect, useRef, type HTMLAttributes, type ReactNode } from 'react';
import { AlertCircle, CheckCircle2, Info, X, AlertTriangle } from 'lucide-react';
import { cva, type VariantProps } from 'class-variance-authority';
import { cn } from '../utils/cn';
import { MotionIcon, infoToneMotion } from './motion';

const infoBarVariants = cva(
    [
        // 基础布局
        'pointer-events-auto relative flex w-full items-start gap-3 overflow-hidden',
        'rounded-md border px-3.5 py-3 shadow-popover backdrop-blur-sm',
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
    title: ReactNode;
    content?: ReactNode;
    autoDismissMs?: number;
    onDismiss?: () => void;
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
        const onDismissRef = useRef(onDismiss);
        onDismissRef.current = onDismiss;
        useEffect(() => {
            if (!autoDismissMs || autoDismissMs <= 0) return;
            const id = setTimeout(() => onDismissRef.current?.(), autoDismissMs);
            return () => clearTimeout(id);
        }, [autoDismissMs]);

        const toneKey = tone ?? 'info';
        const Icon = defaultIconFor(toneKey);

        // 默认 visibility:hidden 让 GSAP 的 autoAlpha enter 第一帧不闪。
        return (
            <div
                ref={ref}
                role="alert"
                style={{ visibility: 'hidden', opacity: 0 }}
                className={cn(infoBarVariants({ tone }), className)}
                {...rest}
            >
                <MotionIcon
                    icon={Icon}
                    motion={infoToneMotion(toneKey)}
                    playEnter={false}
                    size={18}
                    strokeWidth={2.2}
                    className={iconVariants({ tone })}
                />
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
