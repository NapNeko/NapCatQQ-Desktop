// Shimmer: skeleton 扫光骨架。GSAP 版。
//
// rich 档启用,其它档退化为静态浅色块。实现:GSAP 控制 backgroundPosition
// 循环。其实 CSS @keyframes 也能跑,但走 GSAP 让档位/速度/总开关都生效。

import { useEffect, useRef } from 'react';
import gsap from 'gsap';
import { useMotion } from '../../../hooks/preferences/useMotion';

interface ShimmerProps {
    className?: string;
    /// 高度,默认 16px
    height?: number;
}

export function Shimmer({ className, height = 16 }: ShimmerProps) {
    const m = useMotion();
    const animated = m.enabled && m.preset.overshoot;
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
        return () => {
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
