// Hello 卡标题：每个字弹起落定，最后 !! 蹦出来；跨时段换问候时再播一次。
// 悬停时 !! 会再跳一下。动效关闭时就是一行静态标题。

import React, { useLayoutEffect, useMemo, useRef } from 'react';
import gsap from 'gsap';
import { useMotion } from '../../../hooks/preferences/useMotion';

interface HeroTitleProps {
    title: string;
    className?: string;
}

export const HeroTitle: React.FC<HeroTitleProps> = ({ title, className }) => {
    const m = useMotion();
    const ref = useRef<HTMLHeadingElement>(null);
    const markRef = useRef<HTMLSpanElement>(null);
    const chars = useMemo(() => Array.from(title), [title]);

    useLayoutEffect(() => {
        const root = ref.current;
        const mark = markRef.current;
        if (!root || !mark) return;
        const letters = Array.from(root.querySelectorAll<HTMLElement>('[data-letter]'));
        const all = [...letters, mark];
        if (!m.enabled) {
            gsap.set(all, { clearProps: 'all' });
            return;
        }
        const tl = gsap.timeline({ delay: 0.05 });
        tl.fromTo(
            letters,
            { y: 24, autoAlpha: 0, rotate: -8 },
            {
                y: 0,
                autoAlpha: 1,
                rotate: 0,
                duration: m.duration('slow') * 1.3,
                ease: m.ease.enter,
                stagger: m.stagger() > 0 ? Math.max(0.05, m.stagger()) : 0,
            },
        );
        tl.fromTo(
            mark,
            { scale: 0, autoAlpha: 0, rotate: 14 },
            {
                scale: 1,
                autoAlpha: 1,
                rotate: 0,
                duration: m.duration('base') * 1.5,
                ease: m.ease.pop,
            },
            '-=0.08',
        );
        return () => {
            tl.kill();
            gsap.set(all, { clearProps: 'all' });
        };
    }, [title, m.enabled, m.duration, m.ease, m.stagger]);

    const bounceMark = () => {
        const mark = markRef.current;
        if (!mark || !m.enabled || m.preset.feel.popPeak === 1) return;
        gsap.killTweensOf(mark);
        gsap.fromTo(
            mark,
            { y: 0 },
            { y: -7, duration: m.duration('fast'), ease: 'power2.out', yoyo: true, repeat: 1 },
        );
    };

    return (
        <h1 ref={ref} className={className} onMouseEnter={bounceMark}>
            {chars.map((c, i) => (
                <span
                    key={`${c}-${i}`}
                    data-letter
                    className="inline-block"
                    style={{ transformOrigin: '20% 100%' }}
                >
                    {c}
                </span>
            ))}
            <span
                ref={markRef}
                className="ml-1.5 inline-block text-[var(--text-hero-accent)]"
                style={{ transformOrigin: '50% 100%' }}
            >
                !!
            </span>
        </h1>
    );
};

export default HeroTitle;
