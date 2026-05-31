// StatusDot: 状态点呼吸。GSAP 版,精细化第二轮。
//
// 改动:
//   - 不再用 cardLift > 0 当 pulsing 启用条件(那是个借的字段),改用 feel.popPeak > 1
//     这种语义更对的字段。elegant 档 popPeak=1 → 呼吸关。
//   - 呼吸幅度按 overshoot 分级:standard 档 0.55↔1,rich 档 0.45↔1 + 同步 scale
//     0.95↔1.05,呼吸更"鼓"。
//   - tone=danger 时用更急的呼吸(单轮时长 × 0.7),给"出问题了"的紧迫感。

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

const PULSING_TONES: ReadonlySet<StatusDotTone> = new Set(['running', 'loading', 'danger']);

export function StatusDot({ tone, size = 8, className }: StatusDotProps) {
    const m = useMotion();
    const ref = useRef<HTMLSpanElement>(null);
    const pulsing = PULSING_TONES.has(tone) && m.enabled && m.preset.feel.popPeak > 1;

    useEffect(() => {
        const el = ref.current;
        if (!el) return;
        if (!pulsing) {
            gsap.set(el, { opacity: 1, scale: 1 });
            return;
        }
        const f = m.preset.feel;
        // danger 比 running/loading 更急(× 0.7)。speed 滑块同步影响。
        const dur =
            (f.breathDuration / Math.max(0.5, m.speed)) * (tone === 'danger' ? 0.7 : 1);
        const tl = gsap.timeline({ repeat: -1, yoyo: true });
        const opacityLow = f.overshoot ? 0.45 : 0.55;
        tl.to(el, {
            opacity: opacityLow,
            // rich 档加 scale 让呼吸"鼓",standard 档不动 scale 只动 opacity。
            scale: f.overshoot ? 0.92 : 1,
            duration: dur / 2,
            ease: 'sine.inOut',
        });
        return () => {
            tl.kill();
        };
    }, [pulsing, tone, m.preset.feel, m.speed]);

    return (
        <span
            ref={ref}
            className={`inline-block rounded-full ${TONE_CLASSES[tone]} ${className ?? ''}`}
            style={{ width: size, height: size, transformOrigin: 'center' }}
        />
    );
}
