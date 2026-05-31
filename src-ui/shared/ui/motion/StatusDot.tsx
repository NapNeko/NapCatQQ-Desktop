// StatusDot: 带呼吸动画的状态点。
//
// running / loading / pulsing 三态会循环呼吸 opacity;其它态静止。
// 标准/丰富档启用呼吸,优雅档退化为静态。

import { motion } from 'framer-motion';
import { useMotion } from '../../../hooks/preferences/useMotion';

export type StatusDotTone =
    | 'success'   // 静态绿
    | 'running'   // 呼吸绿
    | 'warning'   // 静态橙
    | 'danger'    // 静态红
    | 'idle'      // 静态灰
    | 'loading';  // 呼吸蓝

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
    const pulsing = PULSING_TONES.has(tone) && m.enabled && m.preset.cardLift > 0;
    const cls = `inline-block rounded-full ${TONE_CLASSES[tone]} ${className ?? ''}`;
    const style = { width: size, height: size };

    if (!pulsing) {
        return <span className={cls} style={style} />;
    }

    return (
        <motion.span
            className={cls}
            style={style}
            animate={{ opacity: [0.55, 1, 0.55] }}
            transition={{
                duration: 1.6 / m.speed,
                repeat: Infinity,
                ease: 'easeInOut',
            }}
        />
    );
}
