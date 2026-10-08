// 启动屏第一至三幕（火种、诞生、成形）：进场时间线。
// 从 StartupSplash 主件搬出：整段是纯 GSAP 时序编排，只依赖元素 ref、动效环境
// 与两档降级开关，进场完成/降级静态时通过 onEnterDone 通知主件进入待机幕。

import { useEffect } from 'react';
import gsap from 'gsap';
import { BAR_ENTER, SUB_TEXT } from '../../core/domain/bootstrap/splashSpec';
import type { SplashFxContext } from './splashRefs';

export function useSplashEnter(ctx: SplashFxContext, onEnterDone: (done: boolean) => void): void {
    const { refs, motion, isRich, flourish } = ctx;
    const {
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
    } = refs;

    useEffect(() => {
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
        const spark = sparkRef.current;
        const focusRing = focusRingRef.current;
        const shock = shockRef.current;
        const rim = rimRef.current;
        const version = versionRef.current;
        const shine = shineRef.current;
        const particles = particleRefs.current.filter(Boolean) as HTMLSpanElement[];
        const rings = ringRefs.current.filter(Boolean) as HTMLDivElement[];
        if (!root || !logo || !logoWrap || !logoBox || !title || !sub || !bar) return;

        const rootFx = [spark, focusRing, shock, rim].filter(Boolean) as HTMLDivElement[];

        if (!motion.enabled) {
            gsap.set(
                [
                    stage,
                    logoWrap,
                    logoBox,
                    logo,
                    title,
                    sub,
                    subText,
                    bar,
                    barTrack,
                    version,
                ].filter(Boolean),
                {
                    autoAlpha: 1,
                    y: 0,
                    scale: 1,
                    scaleX: 1,
                    clearProps: 'filter,letterSpacing',
                },
            );
            if (subText) subText.textContent = SUB_TEXT.prepare;
            if (glow) gsap.set(glow, { autoAlpha: 0.35 });
            if (aurora) gsap.set(aurora, { autoAlpha: 0.7 });
            if (brandPulse) gsap.set(brandPulse, { autoAlpha: 0.3, scale: 1 });
            gsap.set(rootFx, { autoAlpha: 0 });
            gsap.set(rings, { autoAlpha: 0.6, scale: 1, rotation: 0 });
            gsap.set(particles, { autoAlpha: 0.3, scale: 1, x: 0, y: 0 });
            onEnterDone(true);
            return;
        }

        onEnterDone(false);

        const k = 1 / Math.max(0.5, motion.speed);
        const s = (v: number) => v * k;
        const t = motion.preset.timing;
        const f = motion.preset.feel;
        const enterDur = motion.duration('slow');
        const baseDur = motion.duration('base');
        const fast = motion.duration('fast');
        const richBoost = isRich ? 1 : flourish ? 0.65 : 0.35;

        // 迸发原点取 logo 的纯布局中心；effect 重跑时元素上可能残留上一轮 transform，先清掉再量
        gsap.set([stage, logoWrap, logoBox, ...particles].filter(Boolean), {
            clearProps: 'transform',
        });
        const logoRect = logoBox.getBoundingClientRect();
        const cx = logoRect.left + logoRect.width / 2;
        const cy = logoRect.top + logoRect.height / 2;
        const burstOffsets = particles.map((p) => {
            const r = p.getBoundingClientRect();
            return { dx: cx - (r.left + r.width / 2), dy: cy - (r.top + r.height / 2) };
        });

        gsap.set(root, { autoAlpha: 1, clearProps: 'clipPath' });
        if (stage) gsap.set(stage, { scale: flourish ? 1.05 : 1, transformOrigin: '50% 45%' });
        gsap.set(rootFx, { left: cx, top: cy, autoAlpha: 0 });
        if (spark) gsap.set(spark, { scale: 0 });
        if (focusRing) gsap.set(focusRing, { scale: 2.6 });
        if (shock) gsap.set(shock, { scale: 0.3 });
        gsap.set(logoWrap, {
            autoAlpha: 0,
            scale: flourish ? 0.3 : Math.max(f.popPeak * 0.86, 0.9),
            y: flourish ? 0 : 14,
            transformOrigin: '50% 50%',
        });
        gsap.set(
            logoBox,
            isRich
                ? {
                      autoAlpha: 0,
                      rotationY: -32,
                      rotationX: 16,
                      transformPerspective: 640,
                      filter: 'brightness(2.2)',
                  }
                : { autoAlpha: 0 },
        );
        gsap.set(logo, { autoAlpha: 1 });
        gsap.set(title, {
            autoAlpha: 0,
            y: 12,
            ...(flourish ? { letterSpacing: '0.32em', filter: 'blur(10px)' } : {}),
        });
        if (subText) {
            subText.textContent = SUB_TEXT.wake;
            gsap.set(subText, { autoAlpha: 1, y: 0 });
        }
        gsap.set(sub, { autoAlpha: 0, y: 8 });
        gsap.set(bar, {
            autoAlpha: 0,
            scaleX: 0,
            transformOrigin: 'left center',
            clearProps: 'filter',
        });
        if (barTrack) gsap.set(barTrack, { autoAlpha: 0, y: 0 });
        if (barShine) gsap.set(barShine, { xPercent: -120, autoAlpha: 0 });
        if (version) gsap.set(version, { autoAlpha: 0, y: 6 });
        if (glow) gsap.set(glow, { autoAlpha: 0, scale: 0.9 });
        if (aurora) gsap.set(aurora, { autoAlpha: 0 });
        if (brandPulse) gsap.set(brandPulse, { autoAlpha: 0, scale: 0.75 });
        if (shine) gsap.set(shine, { left: '-120%', autoAlpha: 0 });
        gsap.set(rings, {
            autoAlpha: 0,
            scale: 0.55,
            rotation: (i: number) => (i % 2 === 0 ? -40 : 40),
        });
        particles.forEach((p, i) => {
            const o = burstOffsets[i];
            gsap.set(
                p,
                flourish
                    ? { autoAlpha: 0, scale: 0, x: o.dx, y: o.dy }
                    : { autoAlpha: 0, scale: 0.6, x: 0, y: 0 },
            );
        });

        const tl = gsap.timeline({
            onComplete: () => {
                gsap.set(logo, { autoAlpha: 1, visibility: 'visible' });
                onEnterDone(true);
            },
        });

        // 底色与"镜头"慢慢推近到位，贯穿整段进场
        if (aurora) {
            tl.to(
                aurora,
                { autoAlpha: 0.55 + 0.45 * richBoost, duration: s(0.7), ease: 'power2.out' },
                0,
            );
        }
        if (glow) {
            tl.to(
                glow,
                {
                    autoAlpha: 0.35 + 0.15 * richBoost,
                    scale: 1.02,
                    duration: enterDur * 1.2,
                    ease: t.ease.enter,
                },
                0,
            );
        }
        if (stage && flourish) {
            tl.to(stage, { scale: 1, duration: s(0.9), ease: 'power2.out' }, 0);
        }

        // 第一幕 火种：中心亮起一粒光并微微抖动，细环从外向内收拢蓄力
        const birthAt = flourish ? s(0.18) : 0;
        if (flourish) {
            if (spark) {
                tl.to(
                    spark,
                    { autoAlpha: 1, scale: 1, duration: s(0.1), ease: 'back.out(2.5)' },
                    0,
                ).to(
                    spark,
                    {
                        keyframes: {
                            opacity: [1, 0.55, 1, 0.7, 1],
                            scale: [1, 0.9, 1.18, 1, 1.25],
                        },
                        duration: s(0.08),
                        ease: 'none',
                    },
                    s(0.1),
                );
            }
            if (focusRing) {
                tl.to(
                    focusRing,
                    { autoAlpha: 0.85, scale: 0.3, duration: s(0.16), ease: 'power3.in' },
                    s(0.02),
                );
            }
        }

        // 第二幕 诞生：火星炸开成一圈冲击波，logo 从火星里长出来翻正，星点顺势迸向四周
        if (flourish) {
            if (spark) {
                tl.to(
                    spark,
                    { scale: 4.5, autoAlpha: 0, duration: s(0.12), ease: 'power2.out' },
                    birthAt,
                );
            }
            if (focusRing) tl.set(focusRing, { autoAlpha: 0 }, birthAt);
            if (shock) {
                tl.fromTo(
                    shock,
                    { scale: 0.3, autoAlpha: 0.8 },
                    { scale: 2.4, autoAlpha: 0, duration: s(0.3), ease: 'power2.out' },
                    birthAt,
                );
            }
        }
        if (particles.length > 0) {
            tl.to(
                particles,
                {
                    autoAlpha: 0.45 + 0.4 * richBoost,
                    scale: 1,
                    x: 0,
                    y: 0,
                    duration: flourish ? s(0.45) : baseDur,
                    ease: flourish ? 'power3.out' : t.ease.enterMicro,
                    stagger: { each: s(0.008), from: 'random' },
                },
                birthAt,
            );
        }
        tl.to(
            logoWrap,
            { autoAlpha: 1, scale: 1, y: 0, duration: enterDur, ease: t.ease.pop },
            birthAt + (flourish ? 0 : s(0.04)),
        );
        tl.to(
            logoBox,
            isRich
                ? {
                      autoAlpha: 1,
                      rotationY: 0,
                      rotationX: 0,
                      filter: 'brightness(1)',
                      clearProps: 'filter',
                      duration: s(0.4),
                      ease: 'back.out(1.6)',
                  }
                : { autoAlpha: 1, duration: enterDur * 0.7, ease: 'power2.out' },
            birthAt + (flourish ? 0 : s(0.04)),
        );
        if (brandPulse && richBoost > 0.25) {
            tl.to(
                brandPulse,
                {
                    autoAlpha: 0.55 * richBoost,
                    scale: 1,
                    duration: enterDur * 0.95,
                    ease: t.ease.enter,
                },
                birthAt + s(0.02),
            );
        }

        // 第三幕 成形：轨道环撑开归位，标题收字距，进度条走到一半
        if (rings.length > 0 && flourish) {
            tl.to(
                rings,
                {
                    autoAlpha: 1,
                    scale: 1,
                    rotation: 0,
                    duration: s(0.32),
                    ease: 'back.out(1.8)',
                    stagger: s(0.04),
                },
                birthAt + s(0.06),
            );
        }
        if (shine && isRich) {
            tl.to(
                shine,
                { left: '140%', autoAlpha: 0.5, duration: baseDur * 0.85, ease: 'power2.inOut' },
                birthAt + s(0.1),
            );
        }
        tl.to(
            title,
            {
                autoAlpha: 1,
                y: 0,
                ...(flourish
                    ? {
                          letterSpacing: '-0.025em',
                          filter: 'blur(0px)',
                          clearProps: 'filter,letterSpacing',
                      }
                    : {}),
                duration: flourish ? s(0.36) : baseDur,
                ease: isRich ? 'expo.out' : t.ease.enter,
            },
            birthAt + s(0.1),
        )
            .to(
                sub,
                { autoAlpha: 1, y: 0, duration: fast, ease: t.ease.enterMicro },
                birthAt + s(0.15),
            )
            .to(
                bar,
                { autoAlpha: 1, scaleX: BAR_ENTER, duration: s(0.25), ease: t.ease.damped },
                birthAt + s(0.17),
            );
        if (barTrack) {
            tl.to(
                barTrack,
                { autoAlpha: 1, duration: fast, ease: 'power2.out' },
                birthAt + s(0.16),
            );
        }
        if (barShine && richBoost > 0.4) {
            tl.to(
                barShine,
                { xPercent: 220, autoAlpha: 0.85, duration: baseDur * 1.1, ease: 'power1.inOut' },
                birthAt + s(0.2),
            );
        }
        if (version) {
            tl.to(
                version,
                { autoAlpha: 1, y: 0, duration: fast, ease: t.ease.enterMicro },
                birthAt + s(0.22),
            );
        }

        const safetyEnterTimer = window.setTimeout(
            () => {
                onEnterDone(true);
            },
            tl.duration() * 1000 + 300,
        );

        return () => {
            window.clearTimeout(safetyEnterTimer);
            tl.kill();
            gsap.killTweensOf(
                [
                    root,
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
                    version,
                    shine,
                    ...rootFx,
                    ...rings,
                    ...particles,
                ].filter(Boolean),
            );
        };
    }, [
        motion.enabled,
        motion.speed,
        motion.level,
        isRich,
        flourish,
        onEnterDone,
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
    ]);
}
