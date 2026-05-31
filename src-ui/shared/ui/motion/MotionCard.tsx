// MotionCard: 卡片 hover 上抬 + 阴影加深的原子件。
//
// 默认 Card 是静态展示件;某些卡片(BotCard / ServerCard / ContainerCard)是
// "可点击有 hover 反馈"的,这种用 MotionCard 包一层。优雅档退化为静态。
//
// 设计取舍:
//   - 不改 Card.tsx 给它加 hover prop(那样会让所有 Card 调用方都重新评估
//     是不是 interactive),而是单独提供 MotionCard 让需要的页面显式开启
//   - 卡片 hover 不弹性,只 translateY + shadow,避免列表大量卡片同时 spring
//     带来抖动感

import { forwardRef } from 'react';
import { motion, type HTMLMotionProps } from 'framer-motion';
import { Card, type CardProps } from '../Card';
import { useMotion } from '../../../hooks/preferences/useMotion';

interface MotionCardProps extends CardProps {
    /// 关闭 hover 上抬。默认开。
    flat?: boolean;
}

export const MotionCard = forwardRef<HTMLDivElement, MotionCardProps>(
    ({ flat, ...cardProps }, ref) => {
        const m = useMotion();
        const lift = m.preset.cardLift;

        if (flat || !m.enabled || lift === 0) {
            return <Card ref={ref} {...cardProps} />;
        }

        const motionProps: HTMLMotionProps<'div'> = {
            whileHover: { y: -lift },
            transition: m.transition('spring'),
        };

        return (
            <motion.div ref={ref} {...motionProps}>
                <Card {...cardProps} />
            </motion.div>
        );
    },
);
MotionCard.displayName = 'MotionCard';
