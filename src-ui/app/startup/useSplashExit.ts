// 启动屏第五幕（出发）：进度冲满、收束、爆发、以 logo 为圆心开洞揭示主界面。
// 从 StartupSplash 主件搬出：exiting 置真后只跑一次的退场时间线，
// notifyReveal 在开洞那一刻回调（宿主提前起播主界面入场），onComplete 走 finish。

import { useEffect } from 'react';
import gsap from 'gsap';
import type { MutableRefObject } from 'react';
import { RING_FX_SIZE, SUB_TEXT } from '../../core/domain/bootstrap/splashSpec';
import { IRIS_SUPPORTED, irisClipPath, swapText } from './splashFx';
import type { SplashFxContext } from './splashRefs';

export function useSplashExit(
    ctx: SplashFxContext,
    exiting: boolean,
    finishedRef: MutableRefObject<boolean>,
    notifyReveal: () => void,
    finish: () => void,
): void {
    const { refs, motion, isRich, flourish } = ctx;
    const {
        rootRef,
        stageRef,
        logoRef,
        logoWrapRef,
        logoBoxRef,
        brandPulseRef,
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
        particleRefs,
        ringRefs,
        sparkleRefs,
    } = refs;

    // 第五幕 出发
    useEffect(() => {
        if (!exiting || finishedRef.current) return;
        const root = rootRef.current;
        const stage = stageRef.current;
        const logo = logoRef.current;
        const logoWrap = logoWrapRef.current;
        const logoBox = logoBoxRef.current;
        const title = titleRef.current;
        const sub = subRef.current;
        const subText = subTextRef.current;
        const bar = barRef.current;
        const barTrack = barTrackRef.current;
        const barShine = barShineRef.current;
        const glow = glowRef.current;
        const aurora = auroraRef.current;
        const brandPulse = brandPulseRef.current;
        const shock = shockRef.current;
        const rim = rimRef.current;
        const version = versionRef.current;
        const particles = particleRefs.current.filter(Boolean) as HTMLSpanElement[];
        const rings = ringRefs.current.filter(Boolean) as HTMLDivElement[];
        const sparkles = sparkleRefs.current.filter(Boolean) as HTMLSpanElement[];
        if (!root) return;

        if (!motion.enabled) {
            finish();
            return;
        }

        const k = 1 / Math.max(0.5, motion.speed);
        const s = (v: number) => v * k;
        const t = motion.preset.timing;
        const exitDur = motion.duration('fast');
        gsap.killTweensOf(
            [
                stage,
                logo,
                logoBox,
                logoWrap,
                title,
                sub,
                subText,
                bar,
                barTrack,
                barShine,
                glow,
                aurora,
                brandPulse,
                shock,
                rim,
                version,
                ...rings,
                ...particles,
                ...sparkles,
            ].filter(Boolean),
        );

        const exitTl = gsap.timeline({ onComplete: finish });

        // 就绪：换词，进度条冲满并亮一下
        if (subText) {
            const swap = swapText(subText, SUB_TEXT.ready, k);
            if (swap) exitTl.add(swap, 0);
        }
        if (bar) {
            exitTl
                .to(bar, { scaleX: 1, autoAlpha: 1, duration: s(0.1), ease: 'power3.out' }, 0)
                .to(
                    bar,
                    { filter: 'brightness(1.6)', duration: s(0.05), ease: 'power2.out' },
                    s(0.06),
                )
                .to(
                    bar,
                    {
                        filter: 'brightness(1)',
                        clearProps: 'filter',
                        duration: s(0.12),
                        ease: 'power2.out',
                    },
                    s(0.11),
                );
        }
        if (barShine && flourish) {
            exitTl.fromTo(
                barShine,
                { xPercent: -120, autoAlpha: 0.9 },
                { xPercent: 220, autoAlpha: 0.9, duration: s(0.14), ease: 'power2.inOut' },
                0,
            );
        }

        if (!flourish) {
            notifyReveal();
            exitTl.to(
                root,
                { autoAlpha: 0, duration: exitDur * 1.2, ease: 'power2.inOut' },
                s(0.12),
            );
            return;
        }

        // 揭示原点取 logo 此刻的实际中心（待机漂移之后）
        const anchor = logoBox ?? logoWrap ?? root;
        const anchorRect = anchor.getBoundingClientRect();
        const cx = anchorRect.left + anchorRect.width / 2;
        const cy = anchorRect.top + anchorRect.height / 2;
        const W = root.clientWidth;
        const H = root.clientHeight;
        gsap.set([shock, rim].filter(Boolean), { left: cx, top: cy });

        // 收束：轨道环加速塌缩，星点被吸回 logo
        const gatherAt = s(0.06);
        if (rings.length > 0) {
            exitTl.to(
                rings,
                {
                    scale: 0.3,
                    autoAlpha: 0,
                    rotation: '+=160',
                    duration: s(0.16),
                    ease: 'power3.in',
                },
                gatherAt,
            );
        }
        if (particles.length > 0) {
            exitTl.to(
                particles,
                {
                    x: (_i: number, el: Element) => {
                        const r = el.getBoundingClientRect();
                        return Number(gsap.getProperty(el, 'x')) + (cx - (r.left + r.width / 2));
                    },
                    y: (_i: number, el: Element) => {
                        const r = el.getBoundingClientRect();
                        return Number(gsap.getProperty(el, 'y')) + (cy - (r.top + r.height / 2));
                    },
                    scale: 0.15,
                    autoAlpha: 0,
                    duration: s(0.18),
                    ease: 'power3.in',
                    stagger: { each: s(0.003), from: 'random' },
                },
                gatherAt,
            );
        }

        // 爆发：logo 蹲起、四角星芒、冲击波外扩、光晕闪一下
        const burstAt = s(0.16);
        if (logoBox && isRich) {
            exitTl
                .to(
                    logoBox,
                    { scale: 1.1, y: -6, rotation: 2.5, duration: s(0.08), ease: 'back.out(2)' },
                    burstAt,
                )
                .to(
                    logoBox,
                    { scale: 1, y: 0, rotation: 0, duration: s(0.08), ease: 'power2.out' },
                    burstAt + s(0.08),
                );
        }
        if (sparkles.length > 0 && isRich) {
            exitTl
                .fromTo(
                    sparkles,
                    { autoAlpha: 0, scale: 0.2, rotation: -25 },
                    {
                        autoAlpha: 1,
                        scale: 1.35,
                        rotation: 45,
                        duration: s(0.11),
                        stagger: s(0.015),
                        ease: 'back.out(3)',
                    },
                    burstAt,
                )
                .to(
                    sparkles,
                    { autoAlpha: 0, scale: 0.4, y: -14, duration: s(0.11), ease: 'power2.in' },
                    burstAt + s(0.1),
                );
        }
        if (shock) {
            exitTl.fromTo(
                shock,
                { scale: 0.4, autoAlpha: 0.9 },
                { scale: 3.8, autoAlpha: 0, duration: s(0.32), ease: 'power2.out' },
                burstAt + s(0.03),
            );
        }
        if (brandPulse) {
            exitTl
                .to(
                    brandPulse,
                    { autoAlpha: 0.95, scale: 1.2, duration: s(0.05), ease: 'power2.out' },
                    burstAt,
                )
                .to(
                    brandPulse,
                    { autoAlpha: 0, scale: 1.7, duration: s(0.24), ease: 'power2.out' },
                    burstAt + s(0.05),
                );
        }

        // 揭示：文字与底色收走，镜头继续推近，以 logo 为圆心开洞，光边贴着洞口跑，主界面从洞里长出来
        const revealAt = burstAt + s(0.08);
        exitTl.call(notifyReveal, undefined, revealAt);
        if (stage) exitTl.to(stage, { scale: 1.1, duration: s(0.3), ease: 'power2.in' }, revealAt);
        if (logoWrap) {
            exitTl.to(
                logoWrap,
                { scale: 1.45, autoAlpha: 0, duration: s(0.14), ease: 'power2.in' },
                revealAt,
            );
        }
        exitTl.to(
            [title, sub, barTrack ?? bar, version].filter(Boolean),
            { autoAlpha: 0, y: -10, duration: s(0.12), ease: t.ease.exit, stagger: s(0.012) },
            revealAt,
        );
        if (glow) exitTl.to(glow, { autoAlpha: 0, duration: s(0.2), ease: 'power2.out' }, revealAt);
        if (aurora)
            exitTl.to(aurora, { autoAlpha: 0, duration: s(0.2), ease: 'power2.out' }, revealAt);

        if (IRIS_SUPPORTED) {
            const maxR = Math.hypot(Math.max(cx, W - cx), Math.max(cy, H - cy)) + 8;
            const iris = { r: 1 };
            exitTl.to(
                iris,
                {
                    r: maxR,
                    duration: s(0.24),
                    ease: 'power3.inOut',
                    onUpdate: () => {
                        root.style.clipPath = irisClipPath(W, H, cx, cy, iris.r);
                        if (rim) {
                            // 光边始终骑在洞口外沿，越开越淡
                            const p = Math.min(1, (iris.r - 1) / (maxR - 1));
                            gsap.set(rim, {
                                scale: ((iris.r + 10) * 2) / RING_FX_SIZE,
                                autoAlpha: 0.95 * Math.pow(1 - p, 0.6),
                            });
                        }
                    },
                },
                revealAt,
            );
        } else {
            exitTl.to(
                root,
                { autoAlpha: 0, duration: s(0.2), ease: 'power2.inOut' },
                revealAt + s(0.06),
            );
        }
    }, [
        exiting,
        motion.enabled,
        motion.speed,
        motion.preset.timing,
        finish,
        notifyReveal,
        isRich,
        flourish,
        finishedRef,
        rootRef,
        stageRef,
        logoRef,
        logoWrapRef,
        logoBoxRef,
        brandPulseRef,
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
        particleRefs,
        ringRefs,
        sparkleRefs,
    ]);
}
