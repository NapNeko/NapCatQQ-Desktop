// 折叠面板进退场：height clip + fade，幅度跟 useMotion 档位走。
// height 不用 rich 的 back.out：会先涨过再回弹，把下面的卡片顶开再抽回去。

import { useLayoutEffect, useRef, type ReactNode } from 'react';
import gsap from 'gsap';
import { ChevronDown } from 'lucide-react';
import { useMotion, type MotionEnv } from '../../../hooks/preferences/useMotion';
import { GsapPresence, type EnterFn, type ExitFn } from './GsapPresence';
import { cn } from '../../utils/cn';

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
    const reversing = el.dataset.expandPhase === 'closing';
    const from = reversing ? el.getBoundingClientRect().height : 0;
    const opacity = reversing ? Number(gsap.getProperty(el, 'opacity')) : 0;
    gsap.set(el, { overflow: 'hidden', height: 'auto', autoAlpha: 0, visibility: 'visible' });
    const h = Math.max(el.scrollHeight, 1);
    el.dataset.expandPhase = 'opening';
    gsap.set(el, { height: from, opacity });
    if (y && !reversing) gsap.set(inner, { y });

    const tl = gsap.timeline({
        onComplete: () => {
            gsap.set(el, { height: 'auto', overflow: 'visible' });
            el.dataset.expandPhase = 'open';
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
    // 收起从屏幕当前帧开始，不能用完整内容高度把正在展开的面板先撑到顶。
    const h = el.getBoundingClientRect().height;
    const y = slideY(env);
    el.dataset.expandPhase = 'closing';
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
    tl.call(() => {
        el.dataset.expandPhase = 'closed';
    });
    return tl;
};

function ExpandContent({ visible, children }: { visible: boolean; children: ReactNode }) {
    const ref = useRef<HTMLDivElement>(null);
    const m = useMotion();

    useLayoutEffect(() => {
        const inner = ref.current;
        const outer = inner?.parentElement;
        if (!inner || !outer || !visible) return;
        let tween: gsap.core.Tween | null = null;
        if (!m.enabled) {
            gsap.set(outer, { height: 'auto', overflow: 'visible' });
            gsap.set(inner, { clearProps: 'transform' });
            outer.dataset.expandPhase = 'open';
        }
        const observer = new ResizeObserver(() => {
            if (!['open', 'resizing'].includes(outer.dataset.expandPhase ?? '')) return;
            const next = inner.getBoundingClientRect().height;
            const previous = Number(outer.dataset.expandH);
            outer.dataset.expandH = String(next);
            if (!m.enabled || !previous || Math.abs(next - previous) < 1) return;
            const from = tween ? outer.getBoundingClientRect().height : previous;
            tween?.kill();
            outer.dataset.expandPhase = 'resizing';
            gsap.set(outer, { height: from, overflow: 'hidden' });
            tween = gsap.to(outer, {
                height: next,
                duration: m.duration('fast'),
                ease: heightEase(m),
                onComplete: () => {
                    tween = null;
                    outer.dataset.expandPhase = 'open';
                    gsap.set(outer, { height: 'auto', overflow: 'visible' });
                },
            });
        });
        observer.observe(inner);
        return () => {
            observer.disconnect();
            tween?.kill();
        };
    }, [visible, m]);

    return (
        <div ref={ref} className="flow-root min-w-0">
            {children}
        </div>
    );
}

export function ExpandPresence({
    visible,
    children,
    keepMounted = false,
    className,
}: {
    visible: boolean;
    children: ReactNode;
    keepMounted?: boolean;
    className?: string;
}) {
    const retained = useRef(children);
    if (visible) retained.current = children;
    return (
        <GsapPresence
            visible={visible}
            keepMounted={keepMounted}
            onEnter={expandEnter}
            onExit={expandExit}
        >
            {/* 不用 h-0 防闪：inline height 一被清掉会瞬间塌成 0，收起没行程。 */}
            <div
                className={cn('min-w-0', className)}
                aria-hidden={!visible || undefined}
                {...(!visible ? { inert: '' } : {})}
                style={{ pointerEvents: visible ? undefined : 'none' }}
            >
                <ExpandContent visible={visible}>
                    {visible ? children : retained.current}
                </ExpandContent>
            </div>
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
    }, [open, m]);

    return (
        <span ref={ref} className="inline-flex shrink-0 text-text-tertiary" aria-hidden>
            <ChevronDown size={size} />
        </span>
    );
}
