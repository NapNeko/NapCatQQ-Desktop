// 弹窗内多步骤内容切换：高度由外层 Dialog 统一过渡，这里只做淡入 + 轻微上移。

import { useGSAP } from '@gsap/react';
import gsap from 'gsap';
import { useRef, type ReactNode } from 'react';
import { useMotion } from '../../../hooks/preferences/useMotion';

export interface DialogStepTransitionProps {
    /** 步骤标识变化时重播进入动画（如 import 的 pick / scan / review）。 */
    stepKey: string;
    children: ReactNode;
    className?: string;
}

export function DialogStepTransition({ stepKey, children, className }: DialogStepTransitionProps) {
    const m = useMotion();
    const rootRef = useRef<HTMLDivElement>(null);

    useGSAP(
        () => {
            const el = rootRef.current;
            if (!el || !m.enabled) {
                if (el) gsap.set(el, { autoAlpha: 1, y: 0 });
                return;
            }
            gsap.fromTo(
                el,
                { autoAlpha: 0, y: 8 },
                {
                    autoAlpha: 1,
                    y: 0,
                    duration: m.duration('base'),
                    ease: m.ease.enter,
                    clearProps: 'transform',
                },
            );
        },
        { dependencies: [stepKey, m.enabled, m.level, m.speed], scope: rootRef },
    );

    return (
        <div ref={rootRef} className={className}>
            {children}
        </div>
    );
}