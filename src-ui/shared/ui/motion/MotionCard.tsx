// MotionCard: 给 Card 加 hover lift。第二轮+精修。
//
// 跟 ListItem 同样的取舍:卡片只 lift + shadow,不 scale 不 brightness。
// 大卡 scale 1.04 在 px-2 容器里被裁切;brightness 叠加 Tailwind hover bg
// 颜色变化会让卡片"突然变白"。

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
            return m.bindHover(el, { scale: 1, brightness: false });
        }, [flat, m.enabled, m.level, m.speed]);

        return <Card ref={setRef} {...cardProps} />;
    },
);
MotionCard.displayName = 'MotionCard';
