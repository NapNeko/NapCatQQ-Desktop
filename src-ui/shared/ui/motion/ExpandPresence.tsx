// 折叠面板进退场：height clip + fade，幅度跟 useMotion 档位走。
// height 不用 rich 的 back.out：会先涨过再回弹，把下面的卡片顶开再抽回去。

import { useLayoutEffect, useRef, type ReactNode } from 'react';
import gsap from 'gsap';
import { ChevronDown } from 'lucide-react';
import { useMotion, type MotionEnv } from '../../../hooks/preferences/useMotion';
import { GsapPresence, type EnterFn, type ExitFn } from './GsapPresence';

function contentEl(el: HTMLElement): HTMLElement {
    return (el.firstElementChild as HTMLElement | null) ?? el;
}

function slideY(env: MotionEnv): number {
    if (env.level === 'elegant') return 0;
    if (env.level === 'rich') return 8;
    return 6;
}

function heightEase(env: MotionEnv): string {
    return env.level === 'rich' ? env.ease.damped : env.ease.enter;
}

const expandEnter: EnterFn = (el, env) => {
    const inner = contentEl(el);
    const y = slideY(env);
    gsap.set(el, { overflow: 'hidden', height: 'auto', autoAlpha: 0, visibility: 'visible' });
    const h = Math.max(el.scrollHeight, 1);
    gsap.set(el, { height: 0 });
    if (y) gsap.set(inner, { y });

    const tl = gsap.timeline({
        onComplete: () => {
            gsap.set(el, { height: 'auto' });
            el.dataset.expandH = String(el.scrollHeight);
            if (y) gsap.set(inner, { clearProps: 'transform' });
        },
    });
    tl.to(el, {
        height: h,
        autoAlpha: 1,
        duration: env.duration(env.level === 'rich' ? 'base' : 'fast'),
        ease: heightEase(env),
    });
    if (y) {
        tl.to(
            inner,
            {
                y: 0,
                duration: env.duration(env.level === 'rich' ? 'base' : 'fast'),
                ease: env.level === 'rich' ? env.ease.enterMicro : env.ease.enter,
            },
            0,
        );
    }
    return tl;
};

const expandExit: ExitFn = (el, env) => {
    const inner = contentEl(el);
    const stored = Number(el.dataset.expandH);
    const h = Math.max(
        el.getBoundingClientRect().height,
        inner.scrollHeight,
        Number.isFinite(stored) ? stored : 0,
        1,
    );
    const y = slideY(env);
    gsap.set(el, { height: h, overflow: 'hidden' });
    const tl = gsap.timeline();
    tl.to(el, {
        height: 0,
        autoAlpha: 0,
        duration: env.duration('fast'),
        ease: env.ease.exit,
    });
    if (y) {
        tl.to(
            inner,
            {
                y: env.level === 'rich' ? -6 : -4,
                duration: env.duration('fast'),
                ease: env.ease.exit,
            },
            0,
        );
    }
    return tl;
};

export function ExpandPresence({
    visible,
    children,
}: {
    visible: boolean;
    children: ReactNode;
}) {
    return (
        <GsapPresence visible={visible} onEnter={expandEnter} onExit={expandExit}>
            {/* 不用 h-0 防闪：inline height 一被清掉会瞬间塌成 0，收起没行程。 */}
            <div className="overflow-hidden">{children}</div>
        </GsapPresence>
    );
}

export function ExpandChevron({ open, size = 14 }: { open: boolean; size?: number }) {
    const ref = useRef<HTMLSpanElement>(null);
    const primed = useRef(false);
    const m = useMotion();

    useLayoutEffect(() => {
        const el = ref.current;
        if (!el) return;
        const rotation = open ? 0 : -90;
        if (!primed.current || !m.enabled) {
            gsap.set(el, { rotation });
            primed.current = true;
            return;
        }
        gsap.to(el, {
            rotation,
            duration: m.duration('fast'),
            ease: m.ease.damped,
        });
    }, [open, m.enabled, m.level, m.speed]);

    return (
        <span ref={ref} className="inline-flex shrink-0 text-text-tertiary" aria-hidden>
            <ChevronDown size={size} />
        </span>
    );
}
