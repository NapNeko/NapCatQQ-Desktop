// 吉祥物头顶的表情贴纸：五个手绘小 SVG，各有各的出场方式。
// 一次只显示一个；zz 会循环到被换掉为止，其余播完自己消失。

import React, { useEffect, useRef, useState } from 'react';
import gsap from 'gsap';
import { useMotion } from '../../../hooks/preferences/useMotion';

export type EmoteKind = 'heart' | 'exclaim' | 'sparkle' | 'sweat' | 'zz';

export interface Emote {
    kind: EmoteKind;
    /** 每次触发换一个 key，同种表情连发也会重播。 */
    key: number;
}

interface MascotEmoteProps {
    emote: Emote | null;
    className?: string;
}

const Heart: React.FC = () => (
    <svg viewBox="0 0 24 24" width="26" height="26" aria-hidden>
        <path
            d="M12 20.5 4.9 13.6a4.4 4.4 0 0 1 6.2-6.2l.9.9.9-.9a4.4 4.4 0 0 1 6.2 6.2z"
            fill="var(--accent-500)"
            stroke="var(--accent-700)"
            strokeWidth="1.2"
            strokeLinejoin="round"
        />
        <path d="M8.2 10.2c.3-1.1 1-1.8 2.1-2.1" fill="none" stroke="#fff" strokeWidth="1.3" strokeLinecap="round" />
    </svg>
);

const Exclaim: React.FC = () => (
    <svg viewBox="0 0 24 24" width="24" height="24" aria-hidden>
        <path
            d="M10.4 3.8h3.2l-.8 10.4h-1.6z"
            fill="var(--brand-500)"
            stroke="var(--brand-700)"
            strokeWidth="1.1"
            strokeLinejoin="round"
        />
        <circle cx="12" cy="18.6" r="1.9" fill="var(--brand-500)" stroke="var(--brand-700)" strokeWidth="1.1" />
    </svg>
);

const Sparkle: React.FC = () => (
    <svg viewBox="0 0 24 24" width="26" height="26" aria-hidden>
        <path
            data-ray
            d="M12 2.5c.6 5.2 4.3 8.9 9.5 9.5-5.2.6-8.9 4.3-9.5 9.5-.6-5.2-4.3-8.9-9.5-9.5 5.2-.6 8.9-4.3 9.5-9.5z"
            fill="var(--amber-500)"
            stroke="var(--amber-600)"
            strokeWidth="0.9"
            strokeLinejoin="round"
        />
        <circle data-dot cx="19.5" cy="5" r="1.4" fill="var(--amber-500)" />
        <circle data-dot cx="4.5" cy="18.5" r="1.1" fill="var(--amber-500)" />
    </svg>
);

const Sweat: React.FC = () => (
    <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden>
        <path
            d="M12 3.5c2.6 4.4 5.2 7.4 5.2 10.6a5.2 5.2 0 0 1-10.4 0c0-3.2 2.6-6.2 5.2-10.6z"
            fill="var(--blue-500)"
            stroke="var(--blue-600)"
            strokeWidth="1.1"
            strokeLinejoin="round"
        />
        <path d="M9.4 14.2c0 1.4.8 2.4 1.9 2.8" fill="none" stroke="#fff" strokeWidth="1.3" strokeLinecap="round" />
    </svg>
);

// 灰字压在浅色天幕上会糊，套一圈卡片底色描边把它抬出来。
const Zz: React.FC = () => (
    <svg viewBox="0 0 32 24" width="32" height="24" aria-hidden style={{ filter: 'drop-shadow(0 0 1.5px var(--surface-hero))' }}>
        <path
            data-z
            d="M4 14.5h6.5L4 21h7"
            fill="none"
            stroke="var(--text-secondary)"
            strokeWidth="2.4"
            strokeLinecap="round"
            strokeLinejoin="round"
        />
        <path
            data-z
            d="M15 6h6l-6 6.5h6.5"
            fill="none"
            stroke="var(--text-secondary)"
            strokeWidth="2.2"
            strokeLinecap="round"
            strokeLinejoin="round"
        />
    </svg>
);

const GLYPH: Record<EmoteKind, React.FC> = {
    heart: Heart,
    exclaim: Exclaim,
    sparkle: Sparkle,
    sweat: Sweat,
    zz: Zz,
};

export const MascotEmote: React.FC<MascotEmoteProps> = ({ emote, className }) => {
    const m = useMotion();
    const ref = useRef<HTMLDivElement>(null);
    const [shown, setShown] = useState<Emote | null>(null);

    useEffect(() => {
        setShown(emote);
    }, [emote]);

    useEffect(() => {
        const el = ref.current;
        if (!el || !shown) return;
        const speed = Math.max(0.5, m.speed);
        const s = (sec: number) => sec / speed;
        const done = () => setShown((cur) => (cur?.key === shown.key ? null : cur));

        if (!m.enabled) {
            gsap.set(el, { autoAlpha: 1, clearProps: 'transform' });
            const id = window.setTimeout(done, shown.kind === 'zz' ? 0 : 1200);
            return () => window.clearTimeout(id);
        }

        const tl = gsap.timeline({ onComplete: shown.kind === 'zz' ? undefined : done });
        gsap.set(el, { clearProps: 'transform' });

        switch (shown.kind) {
            case 'heart':
                tl.fromTo(el, { scale: 0, y: 10, autoAlpha: 0 }, { scale: 1, y: 0, autoAlpha: 1, duration: s(0.32), ease: m.ease.pop })
                    .to(el, { y: -6, duration: s(0.5), ease: 'sine.inOut', yoyo: true, repeat: 1 })
                    .to(el, { y: -22, autoAlpha: 0, scale: 0.8, duration: s(0.6), ease: 'power2.in' });
                break;
            case 'exclaim':
                tl.fromTo(el, { scale: 0, y: 8, autoAlpha: 0 }, { scale: 1.15, y: 0, autoAlpha: 1, duration: s(0.18), ease: 'power3.out' })
                    .to(el, { scale: 1, duration: s(0.25), ease: m.ease.release })
                    .fromTo(el, { x: -3 }, { x: 0, duration: s(0.5), ease: 'ndf-wiggle' }, '<')
                    .to(el, { autoAlpha: 0, y: -8, duration: s(0.35), ease: 'power2.in' }, '+=0.7');
                break;
            case 'sparkle': {
                const ray = el.querySelector('[data-ray]');
                const dots = el.querySelectorAll('[data-dot]');
                tl.fromTo(el, { scale: 0, rotate: -40, autoAlpha: 0 }, { scale: 1, rotate: 0, autoAlpha: 1, duration: s(0.35), ease: m.ease.pop })
                    .to(el, { rotate: 25, duration: s(1.1), ease: 'sine.inOut' }, '<')
                    .to(ray ?? el, { scale: 0.85, duration: s(0.3), yoyo: true, repeat: 3, ease: 'sine.inOut', transformOrigin: '50% 50%' }, '<0.1')
                    .fromTo(dots, { scale: 0 }, { scale: 1, duration: s(0.25), stagger: 0.12, ease: m.ease.pop, transformOrigin: '50% 50%' }, '<0.15')
                    .to(el, { autoAlpha: 0, scale: 0.6, duration: s(0.4), ease: 'power2.in' }, '-=0.1');
                break;
            }
            case 'sweat':
                tl.fromTo(el, { y: -6, autoAlpha: 0, scale: 0.7 }, { y: 4, autoAlpha: 1, scale: 1, duration: s(0.3), ease: 'power2.out' })
                    .to(el, { y: 16, duration: s(0.9), ease: 'power1.in' })
                    .to(el, { autoAlpha: 0, scaleY: 0.6, duration: s(0.25), ease: 'power2.in' }, '-=0.2');
                break;
            case 'zz': {
                const zs = el.querySelectorAll('[data-z]');
                gsap.set(el, { autoAlpha: 1 });
                tl.repeat(-1);
                tl.fromTo(
                    zs,
                    { y: 6, autoAlpha: 0, scale: 0.7 },
                    { y: -10, autoAlpha: 1, scale: 1, duration: s(1.4), ease: 'sine.out', stagger: s(0.9), transformOrigin: '50% 50%' },
                ).to(zs, { y: -18, autoAlpha: 0, duration: s(0.8), ease: 'power1.in', stagger: s(0.9) }, '-=1.2');
                break;
            }
        }
        return () => {
            tl.kill();
        };
    }, [shown, m.enabled, m.speed, m.ease]);

    if (!shown) return null;
    const Glyph = GLYPH[shown.kind];
    return (
        <div ref={ref} className={className} style={{ transformOrigin: '50% 100%' }} aria-hidden>
            <Glyph />
        </div>
    );
};

export default MascotEmote;
