// 启动屏第四幕（待机）：换词、进度推到八成后呼吸、logo 浮动、轨道环慢转、星点漂移。
// 从 StartupSplash 主件搬出：全是 repeat:-1 的循环 tween，进场结束后才起播，
// 每条都绑 visibilitychange 暂停，卸载时逐条解绑并 kill。

import { useEffect } from 'react';
import gsap from 'gsap';
import {
    BAR_IDLE,
    BAR_IDLE_BREATH,
    ORBIT_RINGS,
    SPLASH_PARTICLES,
    SUB_TEXT,
} from '../../core/domain/bootstrap/splashSpec';
import { bindVisibilityPause } from '../../shared/ui/motion/visibilityPause';
import { swapText } from './splashFx';
import type { SplashFxContext } from './splashRefs';

export function useSplashIdle(ctx: SplashFxContext, exiting: boolean, enterDone: boolean): void {
    const { refs, motion, isRich, flourish } = ctx;
    const {
        logoWrapRef,
        brandPulseRef,
        shineRef,
        barRef,
        barShineRef,
        subTextRef,
        particleRefs,
        ringRefs,
    } = refs;

    // 第四幕 待机：换词、进度推到八成后呼吸，logo 浮动、轨道环慢转、星点漂移
    useEffect(() => {
        const logoWrap = logoWrapRef.current;
        const brandPulse = brandPulseRef.current;
        const shine = shineRef.current;
        const bar = barRef.current;
        const barShine = barShineRef.current;
        const subText = subTextRef.current;
        const particles = particleRefs.current.filter(Boolean) as HTMLSpanElement[];
        const rings = ringRefs.current.filter(Boolean) as HTMLDivElement[];
        if (!motion.enabled || exiting || !enterDone) return;

        const k = 1 / Math.max(0.5, motion.speed);
        const loops: Array<gsap.core.Tween | gsap.core.Timeline> = [];

        if (subText) {
            const swap = swapText(subText, SUB_TEXT.prepare, k);
            if (swap) loops.push(swap);
        }
        if (bar) {
            loops.push(
                gsap
                    .timeline()
                    .to(bar, { scaleX: BAR_IDLE, duration: 0.35 * k, ease: 'power2.out' })
                    .to(bar, {
                        scaleX: BAR_IDLE_BREATH,
                        duration: 1.1 * k,
                        ease: 'sine.inOut',
                        yoyo: true,
                        repeat: -1,
                    }),
            );
        }
        if (logoWrap && isRich) {
            loops.push(
                gsap.to(logoWrap, {
                    y: -4,
                    duration: 2.4 * k,
                    ease: 'sine.inOut',
                    yoyo: true,
                    repeat: -1,
                }),
            );
        }
        if (brandPulse && isRich) {
            loops.push(
                gsap.to(brandPulse, {
                    scale: 1.06,
                    opacity: 0.65,
                    duration: 2.8 * k,
                    ease: 'sine.inOut',
                    yoyo: true,
                    repeat: -1,
                }),
            );
        }
        if (shine && isRich) {
            loops.push(
                gsap.timeline({ repeat: -1, repeatDelay: 2.8 * k }).to(shine, {
                    left: '140%',
                    autoAlpha: 0.45,
                    duration: 0.85 * k,
                    ease: 'power2.inOut',
                    onStart: () => {
                        gsap.set(shine, { left: '-120%', autoAlpha: 0 });
                    },
                }),
            );
        }
        if (barShine && flourish) {
            loops.push(
                gsap.fromTo(
                    barShine,
                    { xPercent: -120, autoAlpha: 0.35 },
                    {
                        xPercent: 220,
                        autoAlpha: 0.85,
                        duration: 1.2 * k,
                        ease: 'power1.inOut',
                        repeat: -1,
                        repeatDelay: 0.35 * k,
                    },
                ),
            );
        }
        if (flourish) {
            rings.forEach((ring, i) => {
                const cfg = ORBIT_RINGS[i];
                if (!cfg) return;
                loops.push(
                    gsap.fromTo(
                        ring,
                        { rotation: 0 },
                        {
                            rotation: 360 * cfg.spin,
                            duration: cfg.dur * k,
                            ease: 'none',
                            repeat: -1,
                        },
                    ),
                );
            });
            particles.forEach((p, i) => {
                const cfg = SPLASH_PARTICLES[i];
                if (!cfg) return;
                loops.push(
                    gsap.to(p, {
                        x: cfg.driftX,
                        y: cfg.driftY,
                        opacity: i % 2 === 0 ? 0.35 : 1,
                        duration: cfg.driftDur * k,
                        ease: 'sine.inOut',
                        yoyo: true,
                        repeat: -1,
                    }),
                );
            });
        }

        const unbinds = loops.map((loop) => bindVisibilityPause(loop));
        return () => {
            for (const u of unbinds) u();
            for (const loop of loops) loop.kill();
        };
    }, [
        motion.enabled,
        motion.speed,
        motion.level,
        exiting,
        enterDone,
        isRich,
        flourish,
        logoWrapRef,
        brandPulseRef,
        shineRef,
        barRef,
        barShineRef,
        subTextRef,
        particleRefs,
        ringRefs,
    ]);
}
