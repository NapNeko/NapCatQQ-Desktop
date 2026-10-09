// 弹窗内多步骤内容切换：高度由外层 Dialog 统一过渡，这里只做淡入 + 轻微上移。

import { useLayoutEffect, useRef, type ReactNode } from 'react';
import { useMotion } from '../../../hooks/preferences/useMotion';
import { cssEase } from '../../../core/design/cssEase';

export interface DialogStepTransitionProps {
    /** 步骤标识变化时重播进入动画（如 import 的 pick / scan / review）。 */
    stepKey: string;
    children: ReactNode;
    className?: string;
}

export function DialogStepTransition({ stepKey, children, className }: DialogStepTransitionProps) {
    const m = useMotion();
    const rootRef = useRef<HTMLDivElement>(null);

    const latest = useRef(m);
    latest.current = m;
    useLayoutEffect(() => {
        const el = rootRef.current;
        const env = latest.current;
        if (!el || !env.enabled || typeof el.animate !== 'function') return;
        // 只动合成属性，步骤挂载和表单校验占用主线程时也能继续播放。
        const animation = el.animate(
            [
                { opacity: 0, transform: 'translateY(6px)' },
                { opacity: 1, transform: 'none' },
            ],
            {
                duration: env.duration('base') * 650,
                easing: cssEase(env.ease.enter),
                fill: 'backwards',
            },
        );
        return () => animation.cancel();
    }, [stepKey, m.enabled, m.level, m.speed]);

    return (
        <div ref={rootRef} className={className}>
            {children}
        </div>
    );
}
