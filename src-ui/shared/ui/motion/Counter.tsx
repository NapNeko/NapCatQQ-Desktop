// Counter: 数字 rolling 切换。GSAP 版,精细化第二轮。
//
// 改动:
//   - 启用条件改 feel.popPeak > 1(原来是 overshoot)。standard 档现在也启用,
//     不再 rich 档独占,毕竟"标准"档 popPeak=1.06 的轻反馈对计数变化合适。
//   - 入场曲线统一走 ease.pop:standard 档 ndf-spring,rich 档 ndf-aftershock(余震)。
//   - 出场用 ease.exit 而不是把 exitEase 字段透传。

import { useEffect, useRef } from 'react';
import gsap from 'gsap';
import { useMotion } from '../../../hooks/preferences/useMotion';

interface CounterProps {
    value: number;
    className?: string;
}

export function Counter({ value, className }: CounterProps) {
    const m = useMotion();
    const enabled = m.enabled && m.preset.feel.popPeak > 1;
    const containerRef = useRef<HTMLSpanElement>(null);
    const prevValueRef = useRef<number>(value);

    useEffect(() => {
        const container = containerRef.current;
        if (!container || !enabled) {
            prevValueRef.current = value;
            return;
        }
        if (prevValueRef.current === value) return;

        const newDigit = container.querySelector<HTMLSpanElement>('[data-digit="current"]');
        if (!newDigit) {
            prevValueRef.current = value;
            return;
        }

        const oldClone = newDigit.cloneNode(true) as HTMLSpanElement;
        oldClone.textContent = String(prevValueRef.current);
        oldClone.style.position = 'absolute';
        oldClone.style.left = '0';
        oldClone.style.right = '0';
        oldClone.style.top = '0';
        oldClone.dataset.digit = 'old';
        container.appendChild(oldClone);

        gsap.to(oldClone, {
            yPercent: -110,
            autoAlpha: 0,
            duration: m.duration('base'),
            ease: m.ease.exit,
            onComplete: () => oldClone.remove(),
        });
        gsap.fromTo(
            newDigit,
            { yPercent: 110, autoAlpha: 0 },
            {
                yPercent: 0,
                autoAlpha: 1,
                duration: m.duration('base'),
                ease: m.ease.pop,
            },
        );

        prevValueRef.current = value;
    }, [value, enabled, m]);

    if (!enabled) {
        return <span className={className}>{value}</span>;
    }

    return (
        <span
            ref={containerRef}
            className={`relative inline-block tabular-nums ${className ?? ''}`}
            style={{ overflow: 'hidden' }}
        >
            <span data-digit="current" className="inline-block">
                {value}
            </span>
        </span>
    );
}
