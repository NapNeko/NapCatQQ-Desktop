// PageTransition: 路由级页面过渡。GSAP 版。
//
// 跟 GsapPresence 配合使用,父级控制 visible 切换。本组件只负责描述 enter/exit
// 动画(autoAlpha + 轻 slide-y + 微缩放),不管 mount/unmount(那个是 GsapPresence 的事)。
//
// 风格:rich 档进场用 back.out 让落位带"啪嗒"弹性,标准/优雅档走 power3.out。

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
    return gsap.fromTo(
        el,
        { autoAlpha: 0, y: 12, scale: 0.985 },
        {
            autoAlpha: 1,
            y: 0,
            scale: 1,
            duration: env.duration('slow'),
            ease: env.preset.enterEase,
        },
    );
};

const exit: ExitFn = (el, env) => {
    return gsap.to(el, {
        autoAlpha: 0,
        y: -8,
        scale: 0.99,
        duration: env.duration('fast'),
        ease: env.preset.exitEase,
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

/// 内层 forwardRef 节点。GsapPresence 通过 cloneElement 注入 ref。
/// 默认 autoAlpha:0 让首帧不闪一下白(GSAP autoAlpha=0 = visibility hidden + opacity 0)。
const PageBody = forwardRef<HTMLDivElement, { children: ReactNode; className?: string }>(
    ({ children, className }, ref) => (
        <div ref={ref} className={className} style={{ visibility: 'hidden', opacity: 0 }}>
            {children}
        </div>
    ),
);
PageBody.displayName = 'PageBody';
