// 首屏启动层：在 App 壳就绪前展示品牌动效；尊重 useMotion / prefers-reduced-motion。

import React, { useEffect, useRef, useState } from 'react';
import gsap from 'gsap';
import { useMotion } from '../hooks/preferences/useMotion';
import { APP_VERSION_LABEL } from '../core/domain/app-meta';
import logoSplash from '../assets/logo-72.png?inline';
import { bindVisibilityPause } from '../shared/ui/motion/visibilityPause';

/// 壳已就绪后至少再展示这么久；实际退场还要等进场时间轴播完。
const MIN_VISIBLE_MS = 880;
const MAX_WAIT_MS = 12_000;

const SPLASH_PARTICLES: ReadonlyArray<{
    left: string;
    top: string;
    size: number;
    delay: number;
    accent?: boolean;
}> = [
    { left: '12%', top: '18%', size: 6, delay: 0 },
    { left: '82%', top: '15%', size: 5, delay: 0.3, accent: true },
    { left: '18%', top: '76%', size: 4, delay: 0.7 },
    { left: '86%', top: '72%', size: 7, delay: 0.2, accent: true },
    { left: '50%', top: '8%', size: 4, delay: 1.0 },
    { left: '5%', top: '46%', size: 5, delay: 0.5 },
    { left: '94%', top: '44%', size: 4, delay: 0.8 },
    { left: '34%', top: '86%', size: 5, delay: 0.25, accent: true },
    { left: '68%', top: '82%', size: 4, delay: 0.65 },
    { left: '46%', top: '24%', size: 3, delay: 1.2 },
    { left: '26%', top: '32%', size: 3, delay: 0.4 },
    { left: '74%', top: '36%', size: 4, delay: 0.85, accent: true },
    { left: '64%', top: '64%', size: 3, delay: 0.9 },
    { left: '30%', top: '60%', size: 4, delay: 0.15 },
];

const SPARKLES: ReadonlyArray<{
    top?: string;
    bottom?: string;
    left?: string;
    right?: string;
    size: number;
    color: string;
    char: string;
}> = [
    { top: '-10px', right: '-12px', size: 18, color: 'var(--brand-300, #f59e0b)', char: '✦' },
    { top: '-8px', left: '-10px', size: 14, color: 'var(--brand-400, #a855f7)', char: '✧' },
    { bottom: '-8px', right: '-8px', size: 14, color: 'var(--accent-400, #ec4899)', char: '✧' },
    { bottom: '-10px', left: '-12px', size: 18, color: 'var(--green-400, #10b981)', char: '✦' },
];

export interface StartupSplashProps {
    shellReady: boolean;
    onFinished: () => void;
}

export const StartupSplash: React.FC<StartupSplashProps> = ({ shellReady, onFinished }) => {
    const rootRef = useRef<HTMLDivElement>(null);
    const logoRef = useRef<HTMLImageElement>(null);
    const logoWrapRef = useRef<HTMLDivElement>(null);
    const logoBoxRef = useRef<HTMLDivElement>(null);
    const brandPulseRef = useRef<HTMLDivElement>(null);
    const titleRef = useRef<HTMLHeadingElement>(null);
    const subRef = useRef<HTMLParagraphElement>(null);
    const subTextRef = useRef<HTMLSpanElement>(null);
    const barRef = useRef<HTMLDivElement>(null);
    const barShineRef = useRef<HTMLDivElement>(null);
    const glowRef = useRef<HTMLDivElement>(null);
    const versionRef = useRef<HTMLParagraphElement>(null);
    const shineRef = useRef<HTMLDivElement>(null);
    const particleRefs = useRef<(HTMLSpanElement | null)[]>([]);
    const sparkleRefs = useRef<(HTMLSpanElement | null)[]>([]);
    const motion = useMotion();
    const mountedAt = useRef(typeof performance !== 'undefined' ? performance.now() : 0);
    const [exiting, setExiting] = useState(false);
    const [enterDone, setEnterDone] = useState(false);
    const finishedRef = useRef(false);

    const isRich = motion.level === 'rich';

    useEffect(() => {
        const root = rootRef.current;
        const logo = logoRef.current;
        const logoWrap = logoWrapRef.current;
        const logoBox = logoBoxRef.current;
        const title = titleRef.current;
        const sub = subRef.current;
        const bar = barRef.current;
        const barShine = barShineRef.current;
        const glow = glowRef.current;
        const brandPulse = brandPulseRef.current;
        const version = versionRef.current;
        const shine = shineRef.current;
        const particles = particleRefs.current.filter(Boolean) as HTMLSpanElement[];
        if (!root || !logo || !title || !sub || !bar) return;

        if (!motion.enabled) {
            gsap.set([logoWrap, logoBox, logo, title, sub, bar, version].filter(Boolean), {
                autoAlpha: 1,
                y: 0,
                scale: 1,
                scaleX: 1,
            });
            if (glow) gsap.set(glow, { autoAlpha: 0.35 });
            if (brandPulse) gsap.set(brandPulse, { autoAlpha: 0 });
            particles.forEach((p) => gsap.set(p, { autoAlpha: 0.25 }));
            setEnterDone(true);
            return;
        }

        setEnterDone(false);

        const t = motion.preset.timing;
        const f = motion.preset.feel;
        const enterDur = motion.duration('slow');
        const fast = motion.duration('fast');
        const popPeak = f.popPeak;
        const richBoost = isRich ? 1 : motion.level === 'standard' ? 0.65 : 0.35;

        gsap.set(root, { autoAlpha: 1 });
        if (logoWrap) {
            gsap.set(logoWrap, {
                autoAlpha: 0,
                scale: Math.max(popPeak * 0.86, 0.9),
                y: 14,
                transformOrigin: '50% 50%',
            });
        }
        if (logoBox) gsap.set(logoBox, { autoAlpha: 0 });
        gsap.set(logo, { autoAlpha: 1 });
        gsap.set(title, {
            autoAlpha: 0,
            y: 12,
            ...(isRich ? { filter: 'blur(6px)' } : {}),
        });
        gsap.set(sub, { autoAlpha: 0, y: 8 });
        gsap.set(bar, { autoAlpha: 0, scaleX: 0, transformOrigin: 'left center' });
        if (barShine) gsap.set(barShine, { xPercent: -120, autoAlpha: 0 });
        if (version) gsap.set(version, { autoAlpha: 0, y: 6 });
        if (glow) gsap.set(glow, { autoAlpha: 0, scale: 0.9 });
        if (brandPulse) gsap.set(brandPulse, { autoAlpha: 0, scale: 0.75 });
        if (shine) gsap.set(shine, { left: '-120%', autoAlpha: 0 });
        particles.forEach((p, i) => {
            gsap.set(p, {
                autoAlpha: 0,
                scale: 0,
                x: (i % 2 === 0 ? -1 : 1) * (10 + (i % 3) * 5),
                y: 14 + (i % 4) * 4,
            });
        });

        const tl = gsap.timeline({
            onComplete: () => {
                if (logo) gsap.set(logo, { autoAlpha: 1, visibility: 'visible' });
                setEnterDone(true);
            },
        });

        // 1. 全局背景氛围柔光与星空微粒平滑淡入 (0s)
        if (glow) {
            tl.to(
                glow,
                {
                    autoAlpha: 0.35 + 0.15 * richBoost,
                    scale: 1.02,
                    duration: enterDur * 1.1,
                    ease: t.ease.enter,
                },
                0,
            );
        }
        if (particles.length > 0 && richBoost > 0.2) {
            tl.to(
                particles,
                {
                    autoAlpha: 0.45 + 0.35 * richBoost,
                    scale: 1,
                    x: 0,
                    y: 0,
                    duration: motion.duration('base'),
                    ease: t.ease.enterMicro,
                    stagger: 0.025,
                },
                0.04,
            );
        }

        // 2. Logo 卡片自然弹入 (0.08s)
        if (logoWrap) {
            tl.to(
                logoWrap,
                {
                    autoAlpha: 1,
                    scale: 1,
                    y: 0,
                    duration: enterDur,
                    ease: t.ease.pop,
                },
                0.08,
            );
        }
        if (logoBox) {
            tl.to(
                logoBox,
                {
                    autoAlpha: 1,
                    duration: enterDur * 0.7,
                    ease: 'power2.out',
                },
                0.08,
            );
        }

        // 3. 伴随 Logo 弹出，自然柔和的自然漫反射柔光自然绽放 (0.18s)
        if (brandPulse && richBoost > 0.25) {
            tl.to(
                brandPulse,
                {
                    autoAlpha: 0.55 * richBoost,
                    scale: 1,
                    duration: enterDur * 0.95,
                    ease: t.ease.enter,
                },
                0.18,
            );
        }
        if (shine && isRich) {
            tl.to(
                shine,
                {
                    left: '140%',
                    autoAlpha: 0.5,
                    duration: motion.duration('base') * 0.85,
                    ease: 'power2.inOut',
                },
                0.24,
            );
        }

        // 4. 标题、副标题与进度条随后优雅就位 (0.3s+)
        tl.to(
            title,
            {
                autoAlpha: 1,
                y: 0,
                ...(isRich
                    ? { filter: 'blur(0px)', clearProps: 'filter' }
                    : {}),
                duration: motion.duration('base'),
                ease: t.ease.enter,
            },
            0.3,
        )
            .to(sub, { autoAlpha: 1, y: 0, duration: fast, ease: t.ease.enterMicro }, 0.4)
            .to(
                bar,
                {
                    autoAlpha: 1,
                    scaleX: 1,
                    duration: motion.duration('base') * 1.2,
                    ease: t.ease.damped,
                },
                0.46,
            );

        if (barShine && richBoost > 0.4) {
            tl.to(
                barShine,
                {
                    xPercent: 220,
                    autoAlpha: 0.85,
                    duration: motion.duration('base') * 1.1,
                    ease: 'power1.inOut',
                },
                0.54,
            );
        }
        if (version) {
            tl.to(
                version,
                { autoAlpha: 1, y: 0, duration: fast, ease: t.ease.enterMicro },
                0.6,
            );
        }

        const safetyEnterTimer = window.setTimeout(() => {
            setEnterDone(true);
        }, 1200);

        if (glow && f.overshoot) {
            gsap.to(glow, {
                autoAlpha: 0.55,
                scale: 1.03,
                duration: 2.4 / Math.max(0.5, motion.speed),
                ease: 'sine.inOut',
                yoyo: true,
                repeat: -1,
            });
        }

        return () => {
            window.clearTimeout(safetyEnterTimer);
            tl.kill();
            gsap.killTweensOf(
                [root, logo, logoBox, logoWrap, title, sub, bar, barShine, glow, brandPulse, version, shine, ...particles].filter(
                    Boolean,
                ),
            );
        };
    }, [motion.enabled, motion.speed, motion.level, isRich]);

    useEffect(() => {
        const logoWrap = logoWrapRef.current;
        const brandPulse = brandPulseRef.current;
        const shine = shineRef.current;
        const barShine = barShineRef.current;
        if (!motion.enabled || exiting || !enterDone) return;

        const floatTween =
            logoWrap && isRich
                ? gsap.to(logoWrap, {
                    y: -4,
                    duration: 2.4 / Math.max(0.5, motion.speed),
                    ease: 'sine.inOut',
                    yoyo: true,
                    repeat: -1,
                })
                : null;

        const pulseTween =
            brandPulse && isRich
                ? gsap.to(brandPulse, {
                    scale: 1.06,
                    opacity: 0.65,
                    duration: 2.8 / Math.max(0.5, motion.speed),
                    ease: 'sine.inOut',
                    yoyo: true,
                    repeat: -1,
                })
                : null;

        const shineLoop =
            shine && isRich
                ? gsap.timeline({ repeat: -1, repeatDelay: 2.8 / Math.max(0.5, motion.speed) }).to(shine, {
                    left: '140%',
                    autoAlpha: 0.45,
                    duration: 0.85 / Math.max(0.5, motion.speed),
                    ease: 'power2.inOut',
                    onStart: () => {
                        gsap.set(shine, { left: '-120%', autoAlpha: 0 });
                    },
                })
                : null;

        const barShineLoop =
            barShine && motion.level !== 'elegant'
                ? gsap.fromTo(
                    barShine,
                    { xPercent: -120, autoAlpha: 0.35 },
                    {
                        xPercent: 220,
                        autoAlpha: 0.85,
                        duration: 1.2 / Math.max(0.5, motion.speed),
                        ease: 'power1.inOut',
                        repeat: -1,
                        repeatDelay: 0.35 / Math.max(0.5, motion.speed),
                    },
                )
                : null;

        const unbinds = [
            bindVisibilityPause(floatTween),
            bindVisibilityPause(pulseTween),
            bindVisibilityPause(shineLoop),
            bindVisibilityPause(barShineLoop),
        ];
        return () => {
            for (const u of unbinds) u();
            floatTween?.kill();
            pulseTween?.kill();
            shineLoop?.kill();
            barShineLoop?.kill();
        };
    }, [motion.enabled, motion.speed, motion.level, exiting, enterDone, isRich]);

    useEffect(() => {
        if (!shellReady || !enterDone || exiting || finishedRef.current) return;
        const elapsed = performance.now() - mountedAt.current;
        const wait = Math.max(0, MIN_VISIBLE_MS - elapsed);
        const timer = window.setTimeout(() => setExiting(true), wait);
        const safety = window.setTimeout(() => {
            if (!finishedRef.current) {
                finishedRef.current = true;
                onFinished();
            }
        }, MAX_WAIT_MS);
        return () => {
            window.clearTimeout(timer);
            window.clearTimeout(safety);
        };
    }, [shellReady, enterDone, exiting, onFinished]);

    useEffect(() => {
        if (!exiting || finishedRef.current) return;
        const root = rootRef.current;
        const logo = logoRef.current;
        const logoWrap = logoWrapRef.current;
        const logoBox = logoBoxRef.current;
        const title = titleRef.current;
        const sub = subRef.current;
        const bar = barRef.current;
        const glow = glowRef.current;
        const brandPulse = brandPulseRef.current;
        const version = versionRef.current;
        const particles = particleRefs.current.filter(Boolean) as HTMLSpanElement[];
        const sparkles = sparkleRefs.current.filter(Boolean) as HTMLSpanElement[];
        if (!root) return;

        const finish = () => {
            if (finishedRef.current) return;
            finishedRef.current = true;
            onFinished();
        };

        if (!motion.enabled) {
            finish();
            return;
        }

        const t = motion.preset.timing;
        const exitDur = motion.duration('fast');
        gsap.killTweensOf(
            [logo, logoBox, logoWrap, title, sub, bar, glow, brandPulse, version, ...particles, ...sparkles].filter(Boolean),
        );

        const exitTl = gsap.timeline({ onComplete: finish });

        // 1. 趣味萌猫唤醒动效：Logo 愉悦弹性起跳，四角迸发璀璨星芒 ✨
        if (logoBox && isRich) {
            exitTl.to(
                logoBox,
                {
                    scale: 1.1,
                    y: -6,
                    rotation: 2.5,
                    duration: 0.16 / Math.max(0.5, motion.speed),
                    ease: 'back.out(2)',
                },
                0,
            ).to(
                logoBox,
                {
                    scale: 1,
                    y: 0,
                    rotation: 0,
                    duration: 0.16 / Math.max(0.5, motion.speed),
                    ease: 'power2.out',
                },
                0.16 / Math.max(0.5, motion.speed),
            );
        }

        if (sparkles.length > 0 && isRich) {
            exitTl.fromTo(
                sparkles,
                {
                    autoAlpha: 0,
                    scale: 0.2,
                    rotation: -25,
                },
                {
                    autoAlpha: 1,
                    scale: 1.35,
                    rotation: 45,
                    duration: 0.22 / Math.max(0.5, motion.speed),
                    stagger: 0.03,
                    ease: 'back.out(3)',
                },
                0,
            ).to(
                sparkles,
                {
                    autoAlpha: 0,
                    scale: 0.4,
                    y: -14,
                    duration: 0.22 / Math.max(0.5, motion.speed),
                    ease: 'power2.in',
                },
                0.2 / Math.max(0.5, motion.speed),
            );
        }

        if (subTextRef.current) {
            subTextRef.current.innerText = '✨ 启动就绪，欢迎！';
        }

        // 2. 伴随能量光环涟漪向四周扩散，启动层内容与背景优雅溶图
        const exitStartTime = isRich ? 0.24 / Math.max(0.5, motion.speed) : 0;

        exitTl.to(
            [logoWrap, logoBox, logo, title, sub, bar, version].filter(Boolean),
            {
                autoAlpha: 0,
                y: -12,
                scale: 1.02,
                duration: exitDur * 0.9,
                ease: t.ease.exit,
                stagger: 0.02 / Math.max(0.5, motion.speed),
            },
            exitStartTime,
        );
        if (glow) exitTl.to(glow, { autoAlpha: 0, scale: 1.06, duration: exitDur * 0.8, ease: t.ease.exit }, exitStartTime);
        if (brandPulse) exitTl.to(brandPulse, { autoAlpha: 0, scale: 1.3, duration: exitDur * 0.85, ease: t.ease.exit }, exitStartTime);
        if (particles.length) {
            exitTl.to(
                particles,
                {
                    autoAlpha: 0,
                    scale: 0.3,
                    y: -16,
                    duration: exitDur * 0.7,
                    ease: t.ease.exit,
                    stagger: 0.01,
                },
                exitStartTime,
            );
        }
        // 启动背景画布整体平滑淡出，实现无缝溶图揭示主界面
        exitTl.to(
            root,
            {
                autoAlpha: 0,
                duration: exitDur * 1.1,
                ease: 'power2.inOut',
            },
            exitStartTime + exitDur * 0.15,
        );
    }, [exiting, motion.enabled, motion.speed, motion.preset.timing, onFinished, isRich]);

    return (
        <div
            ref={rootRef}
            className="fixed inset-0 z-[200] flex flex-col items-center justify-center overflow-hidden bg-canvas"
            role="status"
            aria-live="polite"
            aria-busy={!exiting}
        >
            <div
                ref={glowRef}
                className="ndf-canvas-glow pointer-events-none absolute inset-0 opacity-0"
                aria-hidden
            />
            {SPLASH_PARTICLES.map((p, i) => (
                <span
                    key={i}
                    ref={(el) => {
                        particleRefs.current[i] = el;
                    }}
                    className={
                        'ndf-splash-particle opacity-0' +
                        (p.accent ? ' ndf-splash-particle--accent' : '')
                    }
                    style={{
                        left: p.left,
                        top: p.top,
                        width: p.size,
                        height: p.size,
                        animationDelay: `${p.delay}s`,
                    }}
                    aria-hidden
                />
            ))}
            <div className="relative z-10 flex flex-col items-center gap-5 px-8">
                <div ref={logoWrapRef} className="relative flex shrink-0 items-center justify-center opacity-0">
                    {/* 纯正、宽广、超柔和的漫反射环境光晕（无突兀边界与硬圆圈） */}
                    <div
                        ref={brandPulseRef}
                        className="pointer-events-none absolute left-1/2 top-1/2 h-[360px] w-[360px] -translate-x-1/2 -translate-y-1/2 rounded-full opacity-0"
                        style={{
                            background:
                                'radial-gradient(circle, color-mix(in srgb, var(--brand-400) 18%, transparent) 0%, color-mix(in srgb, var(--accent-400) 8%, transparent) 42%, transparent 70%)',
                            filter: 'blur(32px)',
                        }}
                        aria-hidden
                    />

                    {/* Logo 周围趣味萌系星芒（启动完成时欢快迸发） */}
                    {SPARKLES.map((s, idx) => (
                        <span
                            key={idx}
                            ref={(el) => {
                                sparkleRefs.current[idx] = el;
                            }}
                            className="pointer-events-none absolute z-20 select-none font-bold opacity-0"
                            style={{
                                top: s.top,
                                bottom: s.bottom,
                                left: s.left,
                                right: s.right,
                                fontSize: `${s.size}px`,
                                color: s.color,
                                filter: 'drop-shadow(0 0 6px currentColor)',
                            }}
                            aria-hidden
                        >
                            {s.char}
                        </span>
                    ))}

                    {/* Logo 卡片：细腻微投影与高光扫光 */}
                    <div
                        ref={logoBoxRef}
                        className="ndf-splash-logo-shine rounded-2xl ring-1 ring-border-subtle shadow-[0_12px_32px_-4px_rgba(0,0,0,0.35),0_0_28px_-4px_color-mix(in_srgb,var(--brand-400)_25%,transparent)] opacity-0"
                    >
                        <img
                            ref={logoRef}
                            src={logoSplash}
                            alt=""
                            width={72}
                            height={72}
                            className="block h-[72px] w-[72px] rounded-2xl"
                            draggable={false}
                        />
                        <div
                            ref={shineRef}
                            className="ndf-splash-shine-stripe z-[1] mix-blend-overlay"
                            aria-hidden
                        />
                    </div>
                </div>
                <div className="flex flex-col items-center gap-1.5 text-center">
                    <h1
                        ref={titleRef}
                        className="bg-gradient-to-r from-text via-brand-200 to-text bg-clip-text text-xl font-semibold tracking-tight text-transparent drop-shadow-sm"
                    >
                        NapCatQQ Desktop
                    </h1>
                    <div ref={subRef} className="flex items-center gap-2 text-sm text-text-secondary">
                        <span className="ndf-splash-pulse-dot" aria-hidden />
                        <span ref={subTextRef}>正在准备界面…</span>
                    </div>
                </div>
                <div
                    className="relative h-1.5 w-56 overflow-hidden rounded-full bg-border-subtle shadow-inner"
                    aria-hidden
                >
                    <div ref={barRef} className="ndf-splash-laser-bar h-full w-full origin-left rounded-full" />
                    <div
                        ref={barShineRef}
                        className="ndf-splash-bar-shine pointer-events-none absolute inset-y-0 left-0 w-1/3 rounded-full opacity-0"
                    />
                </div>
                <p ref={versionRef} className="text-xs text-text-tertiary tabular-nums">
                    {APP_VERSION_LABEL}
                </p>
            </div>
        </div>
    );
};

export default StartupSplash;