// MotionCard: 给 Card 加 hover lift。GSAP 版,精细化第二轮。
//
// 改动:用 m.bindHover 取代手挂 mouseenter/leave + 直接 gsap.to。bindHover 自动:
//   - 处理 enabled / reduced 短路
//   - 加 boxShadow + brightness(由 feel.shadowBoost / brightness 控制)
//   - 退出时走 damped ease 立即归位,不超调

import { forwardRef, useEffect, useRef } from 'react';
import { Card, type CardProps } from '../Card';
import { useMotion } from '../../../hooks/preferences/useMotion';

interface MotionCardProps extends CardProps {
    /// 关闭 hover 上抬。默认开。
    flat?: boolean;
}

export const MotionCard = forwardRef<HTMLDivElement, MotionCardProps>(
    ({ flat, ...cardProps }, ref) => {
        const m = useMotion();
        const localRef = useRef<HTMLDivElement | null>(null);

        const setRef = (node: HTMLDivElement | null) => {
            localRef.current = node;
            if (typeof ref === 'function') ref(node);
            else if (ref) (ref as React.MutableRefObject<HTMLDivElement | null>).current = node;
        };

        useEffect(() => {
            const el = localRef.current;
            if (!el || flat || !m.enabled || m.preset.feel.cardLift === 0) return;
            return m.bindHover(el);
        }, [flat, m.enabled, m.level, m.speed, m.bindHover, m.preset.feel.cardLift]);

        return <Card ref={setRef} {...cardProps} />;
    },
);
MotionCard.displayName = 'MotionCard';
