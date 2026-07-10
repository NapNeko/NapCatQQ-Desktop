// 顶层消息条原子件。GSAP 版。
//   - tone 决定颜色 + 图标(info / success / warning / danger)
//   - 标题 + 内容(content 可选)
//   - 右上角 close 按钮 + 可选 autoDismissMs 自动消失
//   - 进退场动画走 GSAP,由 InfoBarStack 通过 GsapPresence 管 mount/unmount
//
// 视觉对齐 Fluent InfoBar：不透明分色调底 + 左侧图标井，正文统一主色，避免毛玻璃叠在暖色画布上发糊。
//
// 这一层只管展示,不管"何时该出现"。出现时机由上层 hook 推到
// InfoBarStack(通常监听 store 终态事件)。
//
// 注意:本组件 forwardRef 把 root div 暴露给外部,GsapPresence 才能拿到节点。

import { forwardRef, useEffect, useRef, type HTMLAttributes, type ReactNode } from 'react';
import { AlertCircle, CheckCircle2, Info, X, AlertTriangle } from 'lucide-react';
import { cva } from 'class-variance-authority';
import { cn } from '../utils/cn';
import { MotionIcon, infoToneMotion } from './motion';

const toneClass = {
    info: 'ndf-infobar--info',
    success: 'ndf-infobar--success',
    warning: 'ndf-infobar--warning',
    danger: 'ndf-infobar--danger',
} as const;

const iconWellClass = {
    info: 'ndf-infobar-icon-well--info',
    success: 'ndf-infobar-icon-well--success',
    warning: 'ndf-infobar-icon-well--warning',
    danger: 'ndf-infobar-icon-well--danger',
} as const;

const iconVariants = cva('shrink-0', {
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

function defaultIconFor(tone: 'info' | 'success' | 'warning' | 'danger') {
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

export type InfoBarTone = 'info' | 'success' | 'warning' | 'danger';

export interface InfoBarProps
    extends Omit<HTMLAttributes<HTMLDivElement>, 'title' | 'content'> {
    tone?: InfoBarTone;
    title: ReactNode;
    content?: ReactNode;
    autoDismissMs?: number;
    onDismiss?: () => void;
    onAutoDismiss?: () => void;
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
            onAutoDismiss,
            closable = true,
            className,
            children,
            ...rest
        },
        ref,
    ) => {
        const onDismissRef = useRef(onDismiss);
        onDismissRef.current = onDismiss;
        const onAutoDismissRef = useRef(onAutoDismiss);
        onAutoDismissRef.current = onAutoDismiss;
        useEffect(() => {
            if (!autoDismissMs || autoDismissMs <= 0) return;
            const id = setTimeout(
                () => (onAutoDismissRef.current ?? onDismissRef.current)?.(),
                autoDismissMs,
            );
            return () => clearTimeout(id);
        }, [autoDismissMs]);

        const toneKey = tone ?? 'info';
        const Icon = defaultIconFor(toneKey);

        return (
            <div
                ref={ref}
                role="alert"
                style={{ visibility: 'hidden', opacity: 0 }}
                className={cn(
                    'ndf-infobar pointer-events-auto relative flex w-full items-start gap-3 overflow-hidden px-3.5 py-3',
                    toneClass[toneKey],
                    className,
                )}
                {...rest}
            >
                <div className={cn('ndf-infobar-icon-well', iconWellClass[toneKey])}>
                    <MotionIcon
                        icon={Icon}
                        motion={infoToneMotion(toneKey)}
                        playEnter={false}
                        size={18}
                        strokeWidth={2.2}
                        className={iconVariants({ tone: toneKey })}
                    />
                </div>
                <div className="min-w-0 flex-1 pt-0.5">
                    <div className="text-sm font-semibold leading-tight text-text">{title}</div>
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
                            '-mr-1 -mt-0.5 inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-sm',
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