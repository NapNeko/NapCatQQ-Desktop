// Shimmer: skeleton 扫光骨架。GSAP 版,精细化第二轮。
//
// 改动:启用条件改 feel.overshoot(标记 rich 档),沿用之前的"仅 rich 档动"。
// standard/elegant 档静态色块,避免长时间循环动画消耗注意力。

import { useEffect, useRef } from 'react';
import gsap from 'gsap';
import { useMotion } from '../../../hooks/preferences/useMotion';
import { bindVisibilityPause } from './visibilityPause';

interface ShimmerProps {
    className?: string;
    /// 高度,默认 16px
    height?: number;
}

export function Shimmer({ className, height = 16 }: ShimmerProps) {
    const m = useMotion();
    const animated = m.enabled && m.preset.feel.overshoot;
    const ref = useRef<HTMLSpanElement>(null);

    useEffect(() => {
        const el = ref.current;
        if (!el) return;
        if (!animated) {
            gsap.set(el, { backgroundPosition: '50% 0' });
            return;
        }
        const dur = 1.4 / Math.max(0.5, m.speed);
        const tl = gsap.timeline({ repeat: -1 });
        tl.fromTo(
            el,
            { backgroundPosition: '200% 0' },
            { backgroundPosition: '-200% 0', duration: dur, ease: 'sine.inOut' },
        );
        const unbindVis = bindVisibilityPause(tl);
        return () => {
            unbindVis();
            tl.kill();
        };
    }, [animated, m.speed]);

    return (
        <span
            ref={ref}
            className={`block rounded-sm ${className ?? ''}`}
            style={{
                height,
                background: animated
                    ? 'linear-gradient(90deg, var(--surface-inset) 25%, var(--surface) 50%, var(--surface-inset) 75%)'
                    : 'var(--surface-inset)',
                backgroundSize: animated ? '200% 100%' : undefined,
            }}
        />
    );
}
