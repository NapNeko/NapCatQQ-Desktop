// Shimmer: 渐变扫光骨架。loading 态卡片用,替代 Spinner。
// rich 档启用,其它档退化为静态浅色块。

import { useMotion } from '../../../hooks/preferences/useMotion';

interface ShimmerProps {
    className?: string;
    /// 高度,默认 16px
    height?: number;
}

export function Shimmer({ className, height = 16 }: ShimmerProps) {
    const m = useMotion();
    const animated = m.enabled && m.preset.bouncyOvershoot > 1;

    return (
        <span
            className={`block rounded-sm ${className ?? ''}`}
            style={{
                height,
                background: animated
                    ? 'linear-gradient(90deg, var(--surface-inset) 25%, var(--surface) 50%, var(--surface-inset) 75%)'
                    : 'var(--surface-inset)',
                backgroundSize: animated ? '200% 100%' : undefined,
                animation: animated
                    ? `shimmer-sweep ${1.4 / m.speed}s ease-in-out infinite`
                    : undefined,
            }}
        />
    );
}
