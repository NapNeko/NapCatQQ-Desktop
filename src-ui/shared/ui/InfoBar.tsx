// 顶层消息条原子件。GSAP 版。
//   - tone 只决定左侧强调条与图标色(info / success / warning / danger)
//   - 标题 + 内容(content 可选)，正文两行截断，字符串内容悬停看全文
//   - 右上角 close 按钮 + 可选 autoDismissMs 自动消失
//   - 进退场动画走 GSAP,由 InfoBarStack 通过 GsapPresence 管 mount/unmount
//
// 中性卡片底 + 3px 左侧强调条：在暖色画布上与其余 surface 一致，
// 不整版铺色调底。具体颜色在 index.css 的 .ndf-infobar 块。
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

export type { InfoBarTone } from '../../core/domain/ui/infoBarTone';
import type { InfoBarTone } from '../../core/domain/ui/infoBarTone';

export interface InfoBarProps extends Omit<HTMLAttributes<HTMLDivElement>, 'title' | 'content'> {
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
                    'ndf-infobar pointer-events-auto relative flex w-full items-start gap-2.5 overflow-hidden py-2.5 pl-3.5 pr-2',
                    toneClass[toneKey],
                    className,
                )}
                {...rest}
            >
                <MotionIcon
                    icon={Icon}
                    motion={infoToneMotion(toneKey)}
                    playEnter={false}
                    size={15}
                    strokeWidth={2.2}
                    className={iconVariants({ tone: toneKey })}
                />
                <div className="min-w-0 flex-1">
                    <div className="text-[12.5px] font-medium leading-snug text-text">{title}</div>
                    {content && (
                        <div
                            className="mt-0.5 line-clamp-2 break-words text-[12px] leading-relaxed text-text-secondary"
                            title={typeof content === 'string' ? content : undefined}
                        >
                            {content}
                        </div>
                    )}
                    {children && <div className="mt-2 flex items-center gap-1.5">{children}</div>}
                </div>
                {closable && (
                    <button
                        type="button"
                        aria-label="关闭"
                        onClick={() => onDismiss?.()}
                        className={cn(
                            '-mr-0.5 -mt-0.5 inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-sm',
                            'text-text-tertiary transition-colors',
                            'hover:bg-inset hover:text-text',
                            'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-brand',
                        )}
                    >
                        <X size={12} strokeWidth={2.2} />
                    </button>
                )}
            </div>
        );
    },
);
InfoBar.displayName = 'InfoBar';
