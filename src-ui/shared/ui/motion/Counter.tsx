// Counter: 数字 rolling 切换。GSAP 版。
//
// rich 档启用,其它档直接显示新值。实现:value 变化时旧 span 上推消失,新 span
// 从下方滑入。固定容器高度避免布局跳动。

import { useEffect, useRef } from 'react';
import gsap from 'gsap';
import { useMotion } from '../../../hooks/preferences/useMotion';

interface CounterProps {
    value: number;
    className?: string;
}

export function Counter({ value, className }: CounterProps) {
    const m = useMotion();
    const enabled = m.enabled && m.preset.overshoot;
    const containerRef = useRef<HTMLSpanElement>(null);
    const prevValueRef = useRef<number>(value);

    useEffect(() => {
        const container = containerRef.current;
        if (!container || !enabled) {
            prevValueRef.current = value;
            return;
        }
        if (prevValueRef.current === value) return;

        // 上一帧的数字节点上推消失,新数字从下方滑入。我们直接操作 DOM
        // 避免 React 双 children 的复杂 key 处理:旧节点 clone 一个 absolute,
        // 让原节点立即变 value,旧 clone 跑动画完了删除。
        const newDigit = container.querySelector<HTMLSpanElement>('[data-digit="current"]');
        if (!newDigit) {
            prevValueRef.current = value;
            return;
        }

        // clone 旧值显示节点
        const oldClone = newDigit.cloneNode(true) as HTMLSpanElement;
        oldClone.textContent = String(prevValueRef.current);
        oldClone.style.position = 'absolute';
        oldClone.style.left = '0';
        oldClone.style.right = '0';
        oldClone.style.top = '0';
        oldClone.dataset.digit = 'old';
        container.appendChild(oldClone);

        // 旧 clone 上推消失
        gsap.to(oldClone, {
            yPercent: -110,
            autoAlpha: 0,
            duration: m.duration('base'),
            ease: m.preset.exitEase,
            onComplete: () => oldClone.remove(),
        });
        // 新数字从下方滑入
        gsap.fromTo(
            newDigit,
            { yPercent: 110, autoAlpha: 0 },
            {
                yPercent: 0,
                autoAlpha: 1,
                duration: m.duration('base'),
                ease: m.preset.bouncyEase,
            },
        );

        prevValueRef.current = value;
    }, [value, enabled, m.duration, m.preset.exitEase, m.preset.bouncyEase]);

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
