// Tooltip 原子件。第二轮:进退场走 GSAP,从 trigger 方向缩放展开。
//
// Radix Tooltip 提供 data-state=delayed-open/closed,但 mount/unmount 由它管,
// 不能像 Dialog 那样 forceMount。这里在 TooltipContent 内部用 useLayoutEffect 监听
// 自身 data-state 切换:
//   - 切换到 open 时:GSAP fromTo 跑 scale 0.92 + autoAlpha 0 → 1
//   - 切换到 closed 时:Radix 会立即移除 DOM,GSAP 退场来不及跑;改成监听
//     data-state=closed 后让 GSAP 跑 scale + autoAlpha 反转,但 unmount 是 Radix
//     管的我们干涉不了——放弃退场动画,走纯进场。
// transformOrigin 按 side 拍:top→bottom center,bottom→top center,left/right 类似。

import * as RadixTooltip from '@radix-ui/react-tooltip';
import { forwardRef, useLayoutEffect, useRef } from 'react';
import gsap from 'gsap';
import { cn } from '../utils/cn';
import { useMotion } from '../../hooks/preferences/useMotion';

export const TooltipProvider: React.FC<RadixTooltip.TooltipProviderProps> = ({
    delayDuration = 200,
    skipDelayDuration = 80,
    children,
    ...props
}) => (
    <RadixTooltip.Provider
        delayDuration={delayDuration}
        skipDelayDuration={skipDelayDuration}
        {...props}
    >
        {children}
    </RadixTooltip.Provider>
);

export const Tooltip = RadixTooltip.Root;
export const TooltipTrigger = RadixTooltip.Trigger;

function originForSide(side: string): string {
    switch (side) {
        case 'top':
            return '50% 100%';
        case 'bottom':
            return '50% 0%';
        case 'left':
            return '100% 50%';
        case 'right':
            return '0% 50%';
        default:
            return '50% 100%';
    }
}

export const TooltipContent = forwardRef<
    React.ElementRef<typeof RadixTooltip.Content>,
    React.ComponentPropsWithoutRef<typeof RadixTooltip.Content>
>(({ className, sideOffset = 6, side = 'top', style, ...props }, _ref) => {
    const m = useMotion();
    const localRef = useRef<HTMLDivElement | null>(null);
    // 进场只跑一次：父组件轮询重渲时不能再把 style 打回 hidden，否则悬停气泡闪一下就没
    const enteredRef = useRef(false);

    useLayoutEffect(() => {
        const el = localRef.current;
        if (!el) return;
        // 已进场：只保证可见，避免 metrics 轮询触发的 re-render 重播 fromTo
        if (enteredRef.current) {
            gsap.set(el, { autoAlpha: 1, scale: 1 });
            return;
        }
        enteredRef.current = true;
        if (!m.enabled) {
            gsap.set(el, { autoAlpha: 1, scale: 1 });
            return;
        }
        gsap.set(el, { transformOrigin: originForSide(side as string) });
        gsap.fromTo(
            el,
            { autoAlpha: 0, scale: 0.92 },
            {
                autoAlpha: 1,
                scale: 1,
                duration: m.duration('fast'),
                ease: m.ease.enterMicro,
            },
        );
    }, [m, side]);

    return (
        <RadixTooltip.Portal>
            <RadixTooltip.Content
                ref={(node) => {
                    localRef.current = node;
                    // 首次挂载先藏住，由 useLayoutEffect 在 paint 前打开（勿用 React style 常驻 hidden）
                    if (node && !enteredRef.current) {
                        gsap.set(node, { autoAlpha: 0, scale: m.enabled ? 0.92 : 1 });
                    }
                }}
                side={side}
                sideOffset={sideOffset}
                className={cn(
                    'z-50 max-w-xs rounded-sm bg-text px-2.5 py-1.5 text-2xs font-medium text-canvas shadow-popover',
                    className,
                )}
                style={style}
                {...props}
            />
        </RadixTooltip.Portal>
    );
});
TooltipContent.displayName = 'TooltipContent';
