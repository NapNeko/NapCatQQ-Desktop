// PageTransition: 路由级页面过渡。GSAP 版,精细化第二轮。
//
// 改动:
//   - ease 走 m.ease.enter/exit 七档语义
//   - 进场叠 brightness 微闪让"刚加载完"有视觉确认(rich 档明显,standard 弱)
//   - 退场带轻微 blur(rich 档),增强"离场"感

import { forwardRef, type ReactNode } from 'react';
import gsap from 'gsap';
import { GsapPresence, type EnterFn, type ExitFn } from './GsapPresence';

interface PageTransitionProps {
    /// AppNext 用 routeKey 比较切换;每个 PageTransition 实例只对应一个 route。
    /// visible=true 时 enter,从 true→false 时 exit + unmount。
    visible: boolean;
    children: ReactNode;
    className?: string;
    onExited?: () => void;
}

const enter: EnterFn = (el, env) => {
    const f = env.preset.feel;
    return gsap.fromTo(
        el,
        {
            autoAlpha: 0,
            y: 12,
            scale: 0.985,
            // rich 档进场附带 brightness 微闪,新页"亮一下"再恢复。
            filter: f.brightness > 1.02 ? `brightness(${f.brightness * 1.02})` : 'none',
        },
        {
            autoAlpha: 1,
            y: 0,
            scale: 1,
            filter: 'none',
            duration: env.duration('slow'),
            ease: env.ease.enter,
        },
    );
};

const exit: ExitFn = (el, env) => {
    const f = env.preset.feel;
    return gsap.to(el, {
        autoAlpha: 0,
        y: -8,
        scale: 0.99,
        // rich 档退场叠 blur 让旧页"淡远",elegant/standard 不动 filter。
        filter: f.overshoot ? 'blur(2px)' : 'none',
        duration: env.duration('fast'),
        ease: env.ease.exit,
    });
};

export function PageTransition({
    visible,
    children,
    className,
    onExited,
}: PageTransitionProps) {
    return (
        <GsapPresence
            visible={visible}
            onEnter={enter}
            onExit={exit}
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
