// StatusDot: 状态点呼吸。GSAP 版。
//
// running/loading 用 timeline yoyo repeat -1 跑 opacity 呼吸。其它态静态。
// 标准/丰富档启用呼吸,优雅档退化为静态。

import { useEffect, useRef } from 'react';
import gsap from 'gsap';
import { useMotion } from '../../../hooks/preferences/useMotion';

export type StatusDotTone =
    | 'success'
    | 'running'
    | 'warning'
    | 'danger'
    | 'idle'
    | 'loading';

interface StatusDotProps {
    tone: StatusDotTone;
    size?: number;
    className?: string;
}

const TONE_CLASSES: Record<StatusDotTone, string> = {
    success: 'bg-success',
    running: 'bg-success',
    warning: 'bg-warning',
    danger: 'bg-danger',
    idle: 'bg-text-tertiary',
    loading: 'bg-info',
};

const PULSING_TONES: ReadonlySet<StatusDotTone> = new Set(['running', 'loading']);

export function StatusDot({ tone, size = 8, className }: StatusDotProps) {
    const m = useMotion();
    const ref = useRef<HTMLSpanElement>(null);
    const pulsing = PULSING_TONES.has(tone) && m.enabled && m.preset.cardLift > 0;

    useEffect(() => {
        const el = ref.current;
        if (!el) return;
        if (!pulsing) {
            gsap.set(el, { opacity: 1 });
            return;
        }
        const dur = m.preset.breathDuration / Math.max(0.5, m.speed);
        const tl = gsap.timeline({ repeat: -1, yoyo: true });
        tl.to(el, {
            opacity: 0.55,
            duration: dur / 2,
            ease: 'sine.inOut',
        });
        return () => {
            tl.kill();
        };
    }, [pulsing, m.preset.breathDuration, m.speed]);

    return (
        <span
            ref={ref}
            className={`inline-block rounded-full ${TONE_CLASSES[tone]} ${className ?? ''}`}
            style={{ width: size, height: size }}
        />
    );
}
