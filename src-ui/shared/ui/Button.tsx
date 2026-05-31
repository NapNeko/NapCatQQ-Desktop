// 通用按钮原子件。第二轮重写。
//
// 改动:把手挂事件 + 多份 gsap.to 合并到 m.bindHover + m.bindPress 两条 helper。
// hover/press 的 ease 现在按七档语义走(release 用 spring/elastic,damped 立即归位
// 不超调),按钮按下的"啪嗒落位"在 rich 档变得明显。

import { forwardRef, useEffect, useRef, type ButtonHTMLAttributes } from 'react';
import { cva, type VariantProps } from 'class-variance-authority';
import { cn } from '../utils/cn';
import { useMotion } from '../../hooks/preferences/useMotion';

const buttonVariants = cva(
    'inline-flex items-center justify-center gap-2 rounded-sm font-medium ' +
    'transition-colors duration-150 ease-out ' +
    'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-2 focus-visible:ring-offset-canvas ' +
    'disabled:pointer-events-none disabled:opacity-50 select-none',
    {
        variants: {
            variant: {
                primary:
                    'bg-brand text-white shadow-card hover:bg-brand-hover',
                secondary:
                    'bg-surface text-text border border-border-subtle hover:bg-inset hover:border-border',
                ghost:
                    'bg-transparent text-text-secondary hover:bg-inset hover:text-text',
                danger:
                    'bg-danger text-white shadow-card hover:opacity-90',
            },
            size: {
                sm: 'h-7 px-2.5 text-xs',
                md: 'h-9 px-3.5 text-sm',
                lg: 'h-11 px-5 text-base',
                icon: 'h-9 w-9 p-0',
            },
        },
        defaultVariants: {
            variant: 'secondary',
            size: 'md',
        },
    },
);

export interface ButtonProps
    extends ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
    /// 关闭弹性动画。极少数场景可关。默认开。
    flat?: boolean;
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(
    ({ className, variant, size, type = 'button', flat, disabled, ...props }, ref) => {
        const m = useMotion();
        const localRef = useRef<HTMLButtonElement | null>(null);
        const setRef = (node: HTMLButtonElement | null) => {
            localRef.current = node;
            if (typeof ref === 'function') ref(node);
            else if (ref) (ref as React.MutableRefObject<HTMLButtonElement | null>).current = node;
        };

        useEffect(() => {
            const el = localRef.current;
            if (!el || flat || disabled || !m.enabled) return;
            const f = m.preset.feel;
            // hoverScale=1 时不挂 hover(elegant 档),但 press 仍可挂(tap=1 时 bindPress
            // 内部也会跳过实际 to 调用)。先简单起见,两个都挂,内部 helper 自己短路。
            const cleanups = [
                f.hoverScale !== 1
                    ? m.bindHover(el, { lift: null, shadow: false, brightness: false })
                    : () => {},
                f.tapScale !== 1 ? m.bindPress(el) : () => {},
            ];
            return () => cleanups.forEach((fn) => fn());
        }, [
            m.enabled,
            m.level,
            m.speed,
            m.bindHover,
            m.bindPress,
            m.preset.feel.hoverScale,
            m.preset.feel.tapScale,
            flat,
            disabled,
        ]);

        return (
            <button
                ref={setRef}
                type={type}
                disabled={disabled}
                className={cn(buttonVariants({ variant, size }), className)}
                {...props}
            />
        );
    },
);
Button.displayName = 'Button';
