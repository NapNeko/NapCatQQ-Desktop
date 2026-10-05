// StatusDot: 状态点呼吸。
//
// 呼吸是 CSS 关键帧（index.css 的 ndf-status-breathe*），只动 opacity / transform，跑在合成器上；
// 这里只按动效档决定开不开、幅度和单程时长。窗口藏起来时浏览器自己会停 CSS 动画，不用再挂
// visibilitychange。
//   - elegant 档 popPeak=1，不呼吸。
//   - standard 档只降 opacity 到 0.55；rich 档（有 overshoot）降到 0.45 并缩到 0.92，呼吸更鼓。
//   - danger 单轮时长 × 0.7，给「出问题了」的紧迫感；speed 滑块同步影响。

import type { CSSProperties } from 'react';
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
    const f = m.preset.feel;
    const pulsing = PULSING_TONES.has(tone) && m.enabled && f.popPeak > 1;
    const style: CSSProperties & Record<'--ndf-status-dur', string> = {
        width: size,
        height: size,
        transformOrigin: 'center',
        // 单程 = 整轮一半，alternate 往返刚好一轮
        '--ndf-status-dur': `${((f.breathDuration / Math.max(0.5, m.speed)) * (tone === 'danger' ? 0.7 : 1)) / 2}s`,
    };
    const breathe = pulsing ? (f.overshoot ? ' ndf-status-breathe-rich' : ' ndf-status-breathe') : '';

    return (
        <span
            className={`inline-block rounded-full ${TONE_CLASSES[tone]}${breathe} ${className ?? ''}`}
            style={style}
        />
    );
}
