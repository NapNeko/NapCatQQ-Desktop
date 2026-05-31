// PageTransition: 路由级页面过渡。GSAP 版,精细化第二轮+方向感。
//
// direction prop 决定 enter 起点:
//   1  forward(向后翻页) → 新页从右滑入(x=20),旧页向左滑出(x=-12)
//  -1  backward          → 新页从左滑入(x=-20),旧页向右滑出(x=12)
//  0  unknown / 首次     → 不带方向,沿用单纯纵向 fade
//
// rich 档进场叠 brightness 微闪让"刚加载完"有视觉确认;退场叠 blur 让旧页"淡远"。

import { forwardRef, type ReactNode } from 'react';
import gsap from 'gsap';
import { GsapPresence, type EnterFn, type ExitFn } from './GsapPresence';

interface PageTransitionProps {
    visible: boolean;
    children: ReactNode;
    className?: string;
    onExited?: () => void;
    /// 进退场方向:1=向后翻页(右滑入), -1=向前回页(左滑入), 0/undefined=不分方向。
    direction?: -1 | 0 | 1;
}

function makeEnter(dir: number): EnterFn {
    return (el, env) => {
        const f = env.preset.feel;
        const fromX = dir === 0 ? 0 : dir > 0 ? 20 : -20;
        return gsap.fromTo(
            el,
            {
                autoAlpha: 0,
                y: 12,
                x: fromX,
                scale: 0.985,
                filter: f.brightness > 1.02 ? `brightness(${f.brightness * 1.02})` : 'none',
            },
            {
                autoAlpha: 1,
                y: 0,
                x: 0,
                scale: 1,
                filter: 'none',
                duration: env.duration('slow'),
                ease: env.ease.enter,
            },
        );
    };
}

function makeExit(dir: number): ExitFn {
    return (el, env) => {
        const f = env.preset.feel;
        const toX = dir === 0 ? 0 : dir > 0 ? -12 : 12;
        return gsap.to(el, {
            autoAlpha: 0,
            y: -8,
            x: toX,
            scale: 0.99,
            filter: f.overshoot ? 'blur(2px)' : 'none',
            duration: env.duration('fast'),
            ease: env.ease.exit,
        });
    };
}

export function PageTransition({
    visible,
    children,
    className,
    onExited,
    direction = 0,
}: PageTransitionProps) {
    return (
        <GsapPresence
            visible={visible}
            onEnter={makeEnter(direction)}
            onExit={makeExit(direction)}
            onExited={onExited}
        >
            <PageBody className={className}>{children}</PageBody>
        </GsapPresence>
    );
}

const PageBody = forwardRef<HTMLDivElement, { children: ReactNode; className?: string }>(
    ({ children, className }, ref) => (
        <div ref={ref} className={className} style={{ visibility: 'hidden', opacity: 0 }}>
            {children}
        </div>
    ),
);
PageBody.displayName = 'PageBody';
