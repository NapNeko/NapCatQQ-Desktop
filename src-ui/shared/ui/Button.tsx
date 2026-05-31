// 通用按钮原子件。shadcn 模式:cva 定义 variant + size。
//
// 设计取舍:
//   - 不用 Radix Slot 的 asChild 模式
//   - 仅 4 个 variant:primary / secondary / ghost / danger
//   - icon 通过 children 自由排版
//   - hover/tap 弹性走 GSAP + useMotion(),由用户档位驱动。优雅档退化为
//     普通 button(零 GSAP 开销),标准/丰富档挂事件 + gsap.to() 控制 scale。

import { forwardRef, useEffect, useRef, type ButtonHTMLAttributes } from 'react';
import gsap from 'gsap';
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
            if (!el) return;
            const enabled = m.enabled && !flat && !disabled && m.preset.hoverScale !== 1;
            if (!enabled) return;

            const onEnter = () => {
                gsap.to(el, {
                    scale: m.preset.hoverScale,
                    duration: m.duration('fast'),
                    ease: m.preset.hoverEase,
                });
            };
            const onLeave = () => {
                gsap.to(el, {
                    scale: 1,
                    duration: m.duration('fast'),
                    ease: m.preset.hoverEase,
                });
            };
            const onDown = () => {
                gsap.to(el, {
                    scale: m.preset.tapScale,
                    duration: m.duration('fast') * 0.6,
                    ease: 'power2.out',
                });
            };
            const onUp = () => {
                // tap 释放走 bouncy ease,rich 档 elastic 弹回 1。
                gsap.to(el, {
                    scale: m.preset.hoverScale,
                    duration: m.duration('base'),
                    ease: m.preset.bouncyEase,
                });
            };
            el.addEventListener('mouseenter', onEnter);
            el.addEventListener('mouseleave', onLeave);
            el.addEventListener('mousedown', onDown);
            el.addEventListener('mouseup', onUp);
            return () => {
                el.removeEventListener('mouseenter', onEnter);
                el.removeEventListener('mouseleave', onLeave);
                el.removeEventListener('mousedown', onDown);
                el.removeEventListener('mouseup', onUp);
            };
        }, [
            m.enabled,
            flat,
            disabled,
            m.preset.hoverScale,
            m.preset.tapScale,
            m.preset.hoverEase,
            m.preset.bouncyEase,
            m.speed,
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
