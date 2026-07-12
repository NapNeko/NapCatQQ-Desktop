// PageTransition: 路由级页面过渡。GSAP 版,精细化第二轮+方向感。
//
// direction prop 决定 enter 起点:
//   1  forward(向后翻页) → 新页从右滑入(x=20),旧页向左滑出(x=-12)
//  -1  backward          → 新页从左滑入(x=-20),旧页向右滑出(x=12)
//  0  unknown / 首次     → 不带方向,沿用单纯纵向 fade
//
// 性能:整页只动 opacity + transform(x/y);scale 仅 rich 极轻使用。
// 不用 filter blur/brightness。will-change 只在 tween 期间挂上。

import { forwardRef, type ReactNode } from 'react';
import gsap from 'gsap';
import { useMotion } from '../../../hooks/preferences/useMotion';
import { GsapPresence, type EnterFn, type ExitFn } from './GsapPresence';
import { armTransformLayer, disarmTransformLayer } from './layerHints';

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
        const rich = env.level === 'rich';
        const fromX = dir === 0 ? 0 : dir > 0 ? (rich ? 28 : 18) : rich ? -28 : -18;
        const fromY = rich ? 14 : 10;
        armTransformLayer(el);
        return gsap.fromTo(
            el,
            {
                autoAlpha: 0,
                y: fromY,
                x: fromX,
                // 整页 scale 抬巨大合成层;仅 rich 极轻缩放保留弹入感。
                scale: rich ? 0.988 : 1,
                force3D: true,
            },
            {
                autoAlpha: 1,
                y: 0,
                x: 0,
                scale: 1,
                duration: env.duration('slow'),
                ease: env.ease.enter,
                force3D: true,
                onComplete: () => disarmTransformLayer(el),
                onInterrupt: () => disarmTransformLayer(el),
            },
        );
    };
}

function makeExit(dir: number): ExitFn {
    return (el, env) => {
        const rich = env.level === 'rich';
        const toX = dir === 0 ? 0 : dir > 0 ? (rich ? -16 : -10) : rich ? 16 : 10;
        armTransformLayer(el);
        return gsap.to(el, {
            autoAlpha: 0,
            y: rich ? -10 : -6,
            x: toX,
            scale: rich ? 0.992 : 1,
            duration: env.duration('fast'),
            ease: env.ease.exit,
            force3D: true,
            onComplete: () => disarmTransformLayer(el),
            onInterrupt: () => disarmTransformLayer(el),
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
    const { enabled } = useMotion();
    return (
        <GsapPresence
            visible={visible}
            onEnter={enabled ? makeEnter(direction) : undefined}
            onExit={enabled ? makeExit(direction) : undefined}
            onExited={onExited}
        >
            <PageBody className={className} hideUntilEnter={enabled}>
                {children}
            </PageBody>
        </GsapPresence>
    );
}

const PageBody = forwardRef<
    HTMLDivElement,
    { children: ReactNode; className?: string; hideUntilEnter?: boolean }
>(({ children, className, hideUntilEnter }, ref) => (
    <div
        ref={ref}
        className={className}
        // will-change 由 makeEnter/makeExit 临时挂上,避免常驻合成层。
        style={
            hideUntilEnter
                ? { visibility: 'hidden' as const, opacity: 0 }
                : undefined
        }
    >
        {children}
    </div>
));
PageBody.displayName = 'PageBody';
