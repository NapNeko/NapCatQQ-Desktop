// 首屏启动层：在 App 壳就绪前展示品牌动效；尊重 useMotion / prefers-reduced-motion。

import React, { useEffect, useRef, useState } from 'react';
import gsap from 'gsap';
import { useMotion } from '../hooks/preferences/useMotion';
import { APP_VERSION_LABEL } from '../core/domain/app-meta';
import logoPng from '../assets/logo.png';

const MIN_VISIBLE_MS = 680;
const MAX_WAIT_MS = 12_000;

export interface StartupSplashProps {
    shellReady: boolean;
    onFinished: () => void;
}

export const StartupSplash: React.FC<StartupSplashProps> = ({ shellReady, onFinished }) => {
    const rootRef = useRef<HTMLDivElement>(null);
    const logoRef = useRef<HTMLImageElement>(null);
    const titleRef = useRef<HTMLHeadingElement>(null);
    const subRef = useRef<HTMLParagraphElement>(null);
    const barRef = useRef<HTMLDivElement>(null);
    const glowRef = useRef<HTMLDivElement>(null);
    const motion = useMotion();
    const mountedAt = useRef(typeof performance !== 'undefined' ? performance.now() : 0);
    const [exiting, setExiting] = useState(false);
    const finishedRef = useRef(false);

    useEffect(() => {
        const root = rootRef.current;
        const logo = logoRef.current;
        const title = titleRef.current;
        const sub = subRef.current;
        const bar = barRef.current;
        const glow = glowRef.current;
        if (!root || !logo || !title || !sub || !bar) return;

        if (!motion.enabled) {
            gsap.set([logo, title, sub, bar], { autoAlpha: 1, y: 0, scale: 1 });
            if (glow) gsap.set(glow, { autoAlpha: 0.35 });
            return;
        }

        const t = motion.preset.timing;
        const f = motion.preset.feel;
        const enterDur = motion.duration('slow');
        const fast = motion.duration('fast');

        gsap.set(root, { autoAlpha: 1 });
        gsap.set(logo, { autoAlpha: 0, scale: f.popPeak * 0.92, y: 10 });
        gsap.set(title, { autoAlpha: 0, y: 12 });
        gsap.set(sub, { autoAlpha: 0, y: 8 });
        gsap.set(bar, { autoAlpha: 0, scaleX: 0, transformOrigin: 'left center' });
        if (glow) gsap.set(glow, { autoAlpha: 0, scale: 0.92 });

        const tl = gsap.timeline();
        if (glow) {
            tl.to(glow, {
                autoAlpha: 0.4,
                scale: 1,
                duration: enterDur * 1.1,
                ease: t.ease.enter,
            });
        }
        tl.to(
            logo,
            { autoAlpha: 1, scale: 1, y: 0, duration: enterDur, ease: t.ease.pop },
            glow ? '-=0.55' : 0,
        )
            .to(
                title,
                { autoAlpha: 1, y: 0, duration: motion.duration('base'), ease: t.ease.enter },
                '-=0.35',
            )
            .to(sub, { autoAlpha: 1, y: 0, duration: fast, ease: t.ease.enterMicro }, '-=0.2')
            .to(
                bar,
                {
                    autoAlpha: 1,
                    scaleX: 1,
                    duration: motion.duration('base') * 1.2,
                    ease: t.ease.damped,
                },
                '-=0.15',
            );

        if (glow && f.overshoot) {
            gsap.to(glow, {
                autoAlpha: 0.5,
                duration: 2.4 / Math.max(0.5, motion.speed),
                ease: 'sine.inOut',
                yoyo: true,
                repeat: -1,
            });
        }

        return () => {
            tl.kill();
            gsap.killTweensOf([root, logo, title, sub, bar, glow].filter(Boolean));
        };
    }, [motion.enabled, motion.speed, motion.level]);

    useEffect(() => {
        const bar = barRef.current;
        if (!bar || !motion.enabled || exiting) return;
        const tween = gsap.to(bar, {
            scaleX: 0.35,
            duration: 0.9 / Math.max(0.5, motion.speed),
            ease: 'power1.inOut',
            yoyo: true,
            repeat: -1,
            transformOrigin: 'left center',
        });
        return () => {
            tween.kill();
        };
    }, [motion.enabled, motion.speed, exiting]);

    useEffect(() => {
        if (!shellReady || exiting || finishedRef.current) return;
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
    }, [shellReady, exiting, onFinished]);

    useEffect(() => {
        if (!exiting || finishedRef.current) return;
        const root = rootRef.current;
        const logo = logoRef.current;
        const title = titleRef.current;
        const sub = subRef.current;
        const bar = barRef.current;
        const glow = glowRef.current;
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
        gsap.killTweensOf([logo, title, sub, bar, glow].filter(Boolean));

        const exitTl = gsap.timeline({ onComplete: finish });
        exitTl
            .to([logo, title, sub, bar], {
                autoAlpha: 0,
                y: -6,
                duration: exitDur,
                ease: t.ease.exit,
                stagger: 0.04 / Math.max(0.5, motion.speed),
            })
            .to(root, { autoAlpha: 0, duration: exitDur * 0.85, ease: t.ease.exit }, '-=0.12');
        if (glow) {
            exitTl.to(glow, { autoAlpha: 0, duration: exitDur, ease: t.ease.exit }, 0);
        }
    }, [exiting, motion.enabled, motion.speed, motion.preset.timing, onFinished]);

    return (
        <div
            ref={rootRef}
            className="fixed inset-0 z-[200] flex flex-col items-center justify-center bg-canvas"
            role="status"
            aria-live="polite"
            aria-busy={!exiting}
        >
            <div
                ref={glowRef}
                className="ndf-canvas-glow pointer-events-none absolute inset-0 opacity-0"
                aria-hidden
            />
            <div className="relative z-10 flex flex-col items-center gap-5 px-8">
                <img
                    ref={logoRef}
                    src={logoPng}
                    alt=""
                    width={72}
                    height={72}
                    className="h-[72px] w-[72px] rounded-2xl shadow-popover"
                    draggable={false}
                />
                <div className="flex flex-col items-center gap-1 text-center">
                    <h1 ref={titleRef} className="text-xl font-semibold tracking-tight text-text">
                        NapCatQQ Desktop
                    </h1>
                    <p ref={subRef} className="text-sm text-text-secondary">
                        正在准备界面…
                    </p>
                </div>
                <div className="h-1 w-48 overflow-hidden rounded-full bg-border-subtle" aria-hidden>
                    <div ref={barRef} className="h-full w-full origin-left rounded-full bg-brand" />
                </div>
                <p className="text-xs text-text-tertiary tabular-nums">{APP_VERSION_LABEL}</p>
            </div>
        </div>
    );
};

export default StartupSplash;