// 首屏启动层：在 App 壳就绪前展示品牌动效；尊重 useMotion / prefers-reduced-motion。
// 五幕：火种（中心亮起、蓄力环收拢）→ 诞生（火星炸开，logo 从中长出，星点迸发）→ 成形（轨道环、标题、进度到一半）
// → 待机（换词、进度到八成后呼吸）→ 出发（进度冲满、收束、爆发、以 logo 为圆心开洞揭示主界面）。
// 本文件只做状态编排：进场 / 待机 / 退场三条 GSAP 时间线在 startup/ 的 hook 里，
// 静态规格表在 core/domain/bootstrap/splashSpec，JSX 拆给 SplashBackdrop / SplashMascot / SplashStatus。

import React, { useCallback, useEffect, useRef, useState } from 'react';
import { useMotion } from '../hooks/preferences/useMotion';
import { MAX_WAIT_MS, MIN_VISIBLE_BASE_MS } from '../core/domain/bootstrap/splashSpec';
import { SplashBackdrop } from './startup/SplashBackdrop';
import { SplashMascot } from './startup/SplashMascot';
import { SplashStatus } from './startup/SplashStatus';
import { useSplashEnter } from './startup/useSplashEnter';
import { useSplashIdle } from './startup/useSplashIdle';
import { useSplashExit } from './startup/useSplashExit';

export interface StartupSplashProps {
    shellReady: boolean;
    /// 退场开始揭示主界面的那一刻（比 onFinished 早），宿主用它起播主界面入场动画。
    onReveal?: () => void;
    onFinished: () => void;
}

export const StartupSplash: React.FC<StartupSplashProps> = ({
    shellReady,
    onReveal,
    onFinished,
}) => {
    const rootRef = useRef<HTMLDivElement>(null);
    const stageRef = useRef<HTMLDivElement>(null);
    const logoRef = useRef<HTMLImageElement>(null);
    const logoWrapRef = useRef<HTMLDivElement>(null);
    const logoBoxRef = useRef<HTMLDivElement>(null);
    const brandPulseRef = useRef<HTMLDivElement>(null);
    const sparkRef = useRef<HTMLDivElement>(null);
    const focusRingRef = useRef<HTMLDivElement>(null);
    const shockRef = useRef<HTMLDivElement>(null);
    const rimRef = useRef<HTMLDivElement>(null);
    const titleRef = useRef<HTMLHeadingElement>(null);
    const subRef = useRef<HTMLDivElement>(null);
    const subTextRef = useRef<HTMLSpanElement>(null);
    const barRef = useRef<HTMLDivElement>(null);
    const barTrackRef = useRef<HTMLDivElement>(null);
    const barShineRef = useRef<HTMLDivElement>(null);
    const glowRef = useRef<HTMLDivElement>(null);
    const auroraRef = useRef<HTMLDivElement>(null);
    const versionRef = useRef<HTMLParagraphElement>(null);
    const shineRef = useRef<HTMLDivElement>(null);
    const particleRefs = useRef<(HTMLSpanElement | null)[]>([]);
    const ringRefs = useRef<(HTMLDivElement | null)[]>([]);
    const sparkleRefs = useRef<(HTMLSpanElement | null)[]>([]);
    const motion = useMotion();
    const mountedAt = useRef(typeof performance !== 'undefined' ? performance.now() : 0);
    const [exiting, setExiting] = useState(false);
    const [enterDone, setEnterDone] = useState(false);
    const finishedRef = useRef(false);
    const revealedRef = useRef(false);

    const isRich = motion.level === 'rich';
    const flourish = motion.level !== 'elegant';

    const notifyReveal = useCallback(() => {
        if (revealedRef.current) return;
        revealedRef.current = true;
        onReveal?.();
    }, [onReveal]);

    const finish = useCallback(() => {
        if (finishedRef.current) return;
        finishedRef.current = true;
        notifyReveal();
        onFinished();
    }, [notifyReveal, onFinished]);

    // ctx 每轮渲染重建没关系：三条时间线 hook 的依赖数组只放解构出来的稳定 ref
    // 对象与动效环境字段，不放 ctx 本身
    const ctx = {
        refs: {
            rootRef,
            stageRef,
            logoRef,
            logoWrapRef,
            logoBoxRef,
            brandPulseRef,
            sparkRef,
            focusRingRef,
            shockRef,
            rimRef,
            titleRef,
            subRef,
            subTextRef,
            barRef,
            barTrackRef,
            barShineRef,
            glowRef,
            auroraRef,
            versionRef,
            shineRef,
            particleRefs,
            ringRefs,
            sparkleRefs,
        },
        motion,
        isRich,
        flourish,
    };

    useSplashEnter(ctx, setEnterDone);
    useSplashIdle(ctx, exiting, enterDone);
    useSplashExit(ctx, exiting, finishedRef, notifyReveal, finish);

    useEffect(() => {
        if (!shellReady || !enterDone || exiting || finishedRef.current) return;
        const k = motion.enabled ? 1 / Math.max(0.5, motion.speed) : 0;
        const elapsed = performance.now() - mountedAt.current;
        const wait = Math.max(0, MIN_VISIBLE_BASE_MS * k - elapsed);
        const timer = window.setTimeout(() => setExiting(true), wait);
        return () => window.clearTimeout(timer);
    }, [shellReady, enterDone, exiting, motion.enabled, motion.speed]);

    // 兜底：壳就绪后无论退场动画出什么状况，都不能把用户永远关在启动层外
    useEffect(() => {
        if (!shellReady) return;
        const safety = window.setTimeout(finish, MAX_WAIT_MS);
        return () => window.clearTimeout(safety);
    }, [shellReady, finish]);

    return (
        <div
            ref={rootRef}
            className="fixed inset-0 z-[200] flex flex-col items-center justify-center overflow-hidden bg-canvas"
            role="status"
            aria-live="polite"
            aria-busy={!exiting}
        >
            <SplashBackdrop refs={ctx.refs} motionEnabled={motion.enabled} />
            <div ref={stageRef} className="relative z-10 flex flex-col items-center gap-5 px-8">
                <SplashMascot refs={ctx.refs} />
                <SplashStatus refs={ctx.refs} />
            </div>
        </div>
    );
};

export default StartupSplash;
