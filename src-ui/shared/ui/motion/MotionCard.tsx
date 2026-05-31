// MotionCard: 给 Card 加 hover lift。GSAP 版。
//
// 写法:外层 div forwardRef + 内嵌 Card,挂 mouseenter/leave event listener 用
// gsap.to() 控制 y。优雅档/关闭动画/lift=0 时退化为静态 Card,零监听。

import { forwardRef, useEffect, useRef } from 'react';
import gsap from 'gsap';
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
        const localRef = useRef<HTMLDivElement | null>(null);

        // 父级要的 ref 跟我们内部用的是同一个;不暴露 motion 节点,直接给 Card。
        const setRef = (node: HTMLDivElement | null) => {
            localRef.current = node;
            if (typeof ref === 'function') ref(node);
            else if (ref) (ref as React.MutableRefObject<HTMLDivElement | null>).current = node;
        };

        useEffect(() => {
            const el = localRef.current;
            if (!el || flat || !m.enabled || lift === 0) return;
            const onEnter = () => {
                gsap.to(el, {
                    y: -lift,
                    duration: m.duration('fast'),
                    ease: m.preset.hoverEase,
                });
            };
            const onLeave = () => {
                gsap.to(el, {
                    y: 0,
                    duration: m.duration('fast'),
                    ease: m.preset.hoverEase,
                });
            };
            el.addEventListener('mouseenter', onEnter);
            el.addEventListener('mouseleave', onLeave);
            return () => {
                el.removeEventListener('mouseenter', onEnter);
                el.removeEventListener('mouseleave', onLeave);
            };
        }, [flat, m.enabled, lift, m.preset.hoverEase, m.speed]);

        return <Card ref={setRef} {...cardProps} />;
    },
);
MotionCard.displayName = 'MotionCard';
