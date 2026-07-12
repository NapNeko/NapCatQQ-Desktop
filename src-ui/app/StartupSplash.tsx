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
        { left: '14%', top: '22%', size: 6, delay: 0 },
        { left: '78%', top: '18%', size: 5, delay: 0.4, accent: true },
        { left: '22%', top: '72%', size: 4, delay: 0.8 },
        { left: '84%', top: '68%', size: 7, delay: 0.2, accent: true },
        { left: '48%', top: '12%', size: 3, delay: 1.1 },
        { left: '6%', top: '48%', size: 5, delay: 0.6 },
        { left: '92%', top: '42%', size: 4, delay: 0.9 },
        { left: '38%', top: '82%', size: 5, delay: 0.3, accent: true },
        { left: '62%', top: '78%', size: 4, delay: 0.7 },
        { left: '52%', top: '28%', size: 3, delay: 1.3 },
    ];

export interface StartupSplashProps {
    shellReady: boolean;
    onFinished: () => void;
}

export const StartupSplash: React.FC<StartupSplashProps> = ({ shellReady, onFinished }) => {
    const rootRef = useRef<HTMLDivElement>(null);
    const logoRef = useRef<HTMLImageElement>(null);
    const logoWrapRef = useRef<HTMLDivElement>(null);
    const haloRef = useRef<HTMLDivElement>(null);
    const brandPulseRef = useRef<HTMLDivElement>(null);
    const titleRef = useRef<HTMLHeadingElement>(null);
    const subRef = useRef<HTMLParagraphElement>(null);
    const barRef = useRef<HTMLDivElement>(null);
    const barShineRef = useRef<HTMLDivElement>(null);
    const glowRef = useRef<HTMLDivElement>(null);
    const versionRef = useRef<HTMLParagraphElement>(null);
    const shineRef = useRef<HTMLDivElement>(null);
    const particleRefs = useRef<(HTMLSpanElement | null)[]>([]);
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
        const title = titleRef.current;
        const sub = subRef.current;
        const bar = barRef.current;
        const barShine = barShineRef.current;
        const glow = glowRef.current;
        const halo = haloRef.current;
        const brandPulse = brandPulseRef.current;
        const version = versionRef.current;
        const shine = shineRef.current;
        const particles = particleRefs.current.filter(Boolean) as HTMLSpanElement[];
        if (!root || !logo || !title || !sub || !bar) return;

        if (!motion.enabled) {
            gsap.set([logo, title, sub, bar, version].filter(Boolean), {
                autoAlpha: 1,
                y: 0,
                scale: 1,
                scaleX: 1,
            });
            if (glow) gsap.set(glow, { autoAlpha: 0.35 });
            if (halo) gsap.set(halo, { autoAlpha: 0 });
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
        gsap.set(logoWrap ?? logo, { autoAlpha: 1 });
        gsap.set(logo, {
            autoAlpha: 0,
            scale: Math.max(popPeak * 0.88, 0.92),
            y: 14,
            rotation: isRich ? -8 : 0,
            transformOrigin: '50% 50%',
        });
        // blur 仅 rich：滤镜贵，standard 用位移+淡入同样够炫。
        gsap.set(title, {
            autoAlpha: 0,
            y: 14,
            ...(isRich ? { filter: 'blur(6px)' } : {}),
        });
        gsap.set(sub, { autoAlpha: 0, y: 10 });
        gsap.set(bar, { autoAlpha: 0, scaleX: 0, transformOrigin: 'left center' });
        if (barShine) gsap.set(barShine, { xPercent: -120, autoAlpha: 0 });
        if (version) gsap.set(version, { autoAlpha: 0, y: 6 });
        if (glow) gsap.set(glow, { autoAlpha: 0, scale: 0.88 });
        if (halo) gsap.set(halo, { autoAlpha: 0, scale: 0.6, rotation: 0 });
        if (brandPulse) gsap.set(brandPulse, { autoAlpha: 0, scale: 0.85 });
        if (shine) gsap.set(shine, { left: '-120%', autoAlpha: 0 });
        particles.forEach((p, i) => {
            gsap.set(p, {
                autoAlpha: 0,
                scale: 0,
                x: (i % 2 === 0 ? -1 : 1) * (8 + (i % 3) * 4),
                y: 12 + (i % 4) * 3,
            });
        });

        const tl = gsap.timeline();
        if (glow) {
            tl.to(glow, {
                autoAlpha: 0.35 + 0.15 * richBoost,
                scale: 1.02,
                duration: enterDur * 1.15,
                ease: t.ease.enter,
            });
        }
        if (brandPulse && richBoost > 0.4) {
            tl.to(
                brandPulse,
                {
                    autoAlpha: 0.55 * richBoost,
                    scale: 1.15,
                    duration: enterDur * 0.9,
                    ease: t.ease.enter,
                },
                glow ? '-=0.7' : 0,
            );
        }
        if (particles.length > 0 && richBoost > 0.25) {
            tl.to(
                particles,
                {
                    autoAlpha: 0.45 + 0.35 * richBoost,
                    scale: 1,
                    x: 0,
                    y: 0,
                    duration: motion.duration('base'),
                    ease: t.ease.enterMicro,
                    stagger: motion.stagger() || 0.04,
                },
                '-=0.5',
            );
        }
        if (halo && isRich) {
            tl.to(
                halo,
                {
                    autoAlpha: 0.7,
                    scale: 1,
                    duration: enterDur,
                    ease: t.ease.enter,
                },
                '-=0.45',
            );
        }
        tl.to(
            logo,
            {
                autoAlpha: 1,
                scale: 1,
                y: 0,
                rotation: 0,
                duration: enterDur,
                ease: t.ease.pop,
            },
            '-=0.55',
        );
        if (shine && isRich) {
            tl.to(
                shine,
                {
                    left: '140%',
                    autoAlpha: 0.45,
                    duration: motion.duration('base') * 0.9,
                    ease: 'power2.inOut',
                },
                '-=0.35',
            );
        }
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
            '-=0.4',
        )
            .to(sub, { autoAlpha: 1, y: 0, duration: fast, ease: t.ease.enterMicro }, '-=0.22')
            .to(
                bar,
                {
                    autoAlpha: 1,
                    scaleX: 1,
                    duration: motion.duration('base') * 1.25,
                    ease: t.ease.damped,
                },
                '-=0.12',
            );
        if (barShine && richBoost > 0.5) {
            tl.to(
                barShine,
                {
                    xPercent: 220,
                    autoAlpha: 0.9,
                    duration: motion.duration('base') * 1.1,
                    ease: 'power1.inOut',
                },
                '-=0.5',
            );
        }
        if (version) {
            tl.to(
                version,
                { autoAlpha: 1, y: 0, duration: fast, ease: t.ease.enterMicro },
                '-=0.08',
            );
        }

        tl.eventCallback('onComplete', () => {
            gsap.set(logo, { autoAlpha: 1, visibility: 'visible' });
            setEnterDone(true);
        });

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
            tl.kill();
            gsap.killTweensOf(
                [root, logo, logoWrap, title, sub, bar, barShine, glow, halo, brandPulse, version, shine, ...particles].filter(
                    Boolean,
                ),
            );
        };
    }, [motion.enabled, motion.speed, motion.level]);

    useEffect(() => {
        const logoWrap = logoWrapRef.current;
        const halo = haloRef.current;
        const shine = shineRef.current;
        const barShine = barShineRef.current;
        if (!motion.enabled || exiting || !enterDone) return;

        const floatTween =
            logoWrap && isRich
                ? gsap.to(logoWrap, {
                    y: -5,
                    duration: 2.2 / Math.max(0.5, motion.speed),
                    ease: 'sine.inOut',
                    yoyo: true,
                    repeat: -1,
                })
                : null;

        const haloTween =
            halo && isRich
                ? gsap.to(halo, {
                    rotation: 360,
                    duration: 18 / Math.max(0.5, motion.speed),
                    ease: 'none',
                    repeat: -1,
                })
                : null;

        const shineLoop =
            shine && isRich
                ? gsap.timeline({ repeat: -1, repeatDelay: 2.8 / Math.max(0.5, motion.speed) }).to(shine, {
                    left: '140%',
                    autoAlpha: 0.4,
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
                        autoAlpha: 0.75,
                        duration: 1.4 / Math.max(0.5, motion.speed),
                        ease: 'power1.inOut',
                        repeat: -1,
                        repeatDelay: 0.35 / Math.max(0.5, motion.speed),
                    },
                )
                : null;

        const unbinds = [
            bindVisibilityPause(floatTween),
            bindVisibilityPause(haloTween),
            bindVisibilityPause(shineLoop),
            bindVisibilityPause(barShineLoop),
        ];
        return () => {
            for (const u of unbinds) u();
            floatTween?.kill();
            haloTween?.kill();
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
        const title = titleRef.current;
        const sub = subRef.current;
        const bar = barRef.current;
        const glow = glowRef.current;
        const halo = haloRef.current;
        const brandPulse = brandPulseRef.current;
        const version = versionRef.current;
        const particles = particleRefs.current.filter(Boolean);
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
            [logo, title, sub, bar, glow, halo, brandPulse, version, ...particles].filter(Boolean),
        );

        const exitTl = gsap.timeline({ onComplete: finish });
        exitTl.to([logo, title, sub, bar, version].filter(Boolean), {
            autoAlpha: 0,
            y: -8,
            scale: 0.98,
            duration: exitDur,
            ease: t.ease.exit,
            stagger: 0.045 / Math.max(0.5, motion.speed),
        });
        // 根层保持画布不透明，避免退场时透出 WebView 浅底闪白；结束后再卸载 Splash。
        if (glow) exitTl.to(glow, { autoAlpha: 0, scale: 1.08, duration: exitDur, ease: t.ease.exit }, 0);
        if (halo) exitTl.to(halo, { autoAlpha: 0, duration: exitDur * 0.8, ease: t.ease.exit }, 0);
        if (brandPulse) exitTl.to(brandPulse, { autoAlpha: 0, scale: 1.25, duration: exitDur, ease: t.ease.exit }, 0);
        if (particles.length) {
            exitTl.to(
                particles,
                {
                    autoAlpha: 0,
                    scale: 0.6,
                    y: -16,
                    duration: exitDur,
                    ease: t.ease.exit,
                    stagger: 0.02,
                },
                0,
            );
        }
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
            <div
                ref={brandPulseRef}
                className="pointer-events-none absolute left-1/2 top-1/2 h-[min(420px,70vw)] w-[min(420px,70vw)] -translate-x-1/2 -translate-y-1/2 rounded-full opacity-0"
                style={{
                    background:
                        'radial-gradient(circle, color-mix(in srgb, var(--brand-400) 22%, transparent) 0%, transparent 68%)',
                }}
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
                <div ref={logoWrapRef} className="relative flex shrink-0 items-center justify-center">
                    <div
                        ref={haloRef}
                        className="pointer-events-none absolute -inset-5 rounded-[28px] opacity-0"
                        style={{
                            background:
                                'conic-gradient(from 0deg, color-mix(in srgb, var(--brand-400) 0%, transparent), color-mix(in srgb, var(--accent-400) 45%, transparent), color-mix(in srgb, var(--brand-500) 35%, transparent), transparent 75%)',
                            filter: 'blur(10px)',
                        }}
                        aria-hidden
                    />
                    <div className="ndf-splash-logo-shine">
                        <img
                            ref={logoRef}
                            src={logoSplash}
                            alt=""
                            width={72}
                            height={72}
                            className="block h-[72px] w-[72px] rounded-2xl shadow-popover ring-1 ring-border-subtle"
                            draggable={false}
                        />
                        <div
                            ref={shineRef}
                            className="ndf-splash-shine-stripe z-[1] mix-blend-overlay"
                            aria-hidden
                        />
                    </div>
                </div>
                <div className="flex flex-col items-center gap-1 text-center">
                    <h1 ref={titleRef} className="text-xl font-semibold tracking-tight text-text">
                        NapCatQQ Desktop
                    </h1>
                    <p ref={subRef} className="text-sm text-text-secondary">
                        正在准备界面…
                    </p>
                </div>
                <div
                    className="relative h-1.5 w-52 overflow-hidden rounded-full bg-border-subtle"
                    aria-hidden
                >
                    <div ref={barRef} className="h-full w-full origin-left rounded-full bg-brand" />
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