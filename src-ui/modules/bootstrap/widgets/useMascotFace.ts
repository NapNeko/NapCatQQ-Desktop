// 给注入的吉祥物 SVG 装上表情：三双眼睛各自眨，眼珠跟鼠标，还有几种一次性表情。
// 表情动画和眨眼都改 scaleY，跑表情时先把眨眼停了，做完再放开，免得两边打架。

import { useEffect, useMemo, useRef, type RefObject } from 'react';
import gsap from 'gsap';
import type { MotionEnv } from '../../../hooks/preferences/useMotion';
import { bindVisibilityPause } from '../../../shared/ui/motion/visibilityPause';
import { rigMascot, type MascotRig } from './mascotRig';

export interface MascotFace {
    /** 眼珠往 (nx, ny) 看，取值 -1..1。 */
    lookAt: (nx: number, ny: number) => void;
    /** 被戳：眯眼笑 + 腮红鼓一下。 */
    squint: () => void;
    /** 出事了：眼睛睁大。 */
    widen: () => void;
    /** 连戳转晕：眼睛闭成线来回晃，腮红涨红。 */
    dizzy: () => void;
    /** 打瞌睡 / 醒来。 */
    doze: (on: boolean) => void;
}

const NOOP_FACE: MascotFace = {
    lookAt: () => { },
    squint: () => { },
    widen: () => { },
    dizzy: () => { },
    doze: () => { },
};

const EYE_ORIGIN = { transformOrigin: '50% 50%' } as const;

/// 眨眼：闭得快、开得慢；每次间隔随机，四分之一概率紧接着再眨一次。
function blinkLoop(els: SVGGraphicsElement[], min: number, max: number): gsap.core.Timeline {
    const tl = gsap.timeline({
        repeat: -1,
        repeatDelay: gsap.utils.random(min, max),
        onRepeat() {
            tl.repeatDelay(Math.random() < 0.25 ? 0.16 : gsap.utils.random(min, max));
        },
    });
    tl.to(els, { scaleY: 0.08, duration: 0.07, ease: 'power2.in', ...EYE_ORIGIN }).to(els, {
        scaleY: 1,
        duration: 0.14,
        ease: 'power2.out',
    });
    return tl;
}

export function useMascotFace(bodyRef: RefObject<HTMLElement>, m: MotionEnv): MascotFace {
    const rigRef = useRef<MascotRig | null>(null);
    const blinksRef = useRef<gsap.core.Timeline[]>([]);
    // 正在跑的一次性表情。新表情来了先杀它，别用 killTweensOf：那会把眨眼循环里的 tween 一起杀掉。
    const exprRef = useRef<gsap.core.Timeline | null>(null);
    const dozingRef = useRef(false);
    const lookRef = useRef<{ x: (v: number) => void; y: (v: number) => void } | null>(null);

    const enabled = m.enabled;
    const lively = enabled && m.level !== 'elegant';

    useEffect(() => {
        const body = bodyRef.current;
        if (!body || !enabled) return;
        const rig = rigMascot(body);
        if (rig.eyes.length === 0) return;
        rigRef.current = rig;

        const all = [...rig.eyes, ...rig.cheeks, ...rig.heldCatEyes, ...rig.floorCatEyes, ...rig.bow];
        gsap.set(all, EYE_ORIGIN);

        const blinks = [blinkLoop(rig.eyes, 2.6, 5.4)];
        if (lively) {
            blinks.push(blinkLoop(rig.heldCatEyes, 3.5, 7.5), blinkLoop(rig.floorCatEyes, 4, 9));
            lookRef.current = {
                x: gsap.quickTo(rig.eyes, 'x', { duration: 0.5, ease: 'power3.out' }),
                y: gsap.quickTo(rig.eyes, 'y', { duration: 0.5, ease: 'power3.out' }),
            };
        }
        blinksRef.current = blinks;
        const unbind = blinks.map((tl) => bindVisibilityPause(tl));

        return () => {
            unbind.forEach((fn) => fn());
            blinks.forEach((tl) => tl.kill());
            exprRef.current?.kill();
            exprRef.current = null;
            gsap.killTweensOf(all);
            gsap.set(all, { clearProps: 'transform' });
            blinksRef.current = [];
            rigRef.current = null;
            lookRef.current = null;
            dozingRef.current = false;
        };
    }, [bodyRef, enabled, lively]);

    return useMemo<MascotFace>(() => {
        if (!enabled) return NOOP_FACE;

        const speed = Math.max(0.5, m.speed);
        const pauseBlink = () => blinksRef.current[0]?.pause();
        const resumeBlink = () => {
            if (!dozingRef.current) blinksRef.current[0]?.play();
        };
        // 表情打断眨眼：先停、跑完再放；眼睛回到 scaleY 1 由表情自己保证。
        const expression = (build: (rig: MascotRig, tl: gsap.core.Timeline) => void) => {
            const rig = rigRef.current;
            if (!rig || dozingRef.current) return;
            // 先杀旧表情再停眨眼：kill 会触发旧表情的 onInterrupt 把眨眼放开。
            exprRef.current?.kill();
            pauseBlink();
            const tl = gsap.timeline({ onComplete: resumeBlink, onInterrupt: resumeBlink });
            exprRef.current = tl;
            build(rig, tl);
        };

        return {
            lookAt: (nx, ny) => {
                const look = lookRef.current;
                if (!look) return;
                look.x(nx * 10);
                look.y(ny * 5);
            },
            squint: () =>
                expression((rig, tl) => {
                    tl.to(rig.eyes, { scaleY: 0.3, duration: 0.12 / speed, ease: 'power2.in' })
                        .to(rig.cheeks, { scale: 1.55, duration: 0.18 / speed, ease: m.ease.pop }, 0)
                        .to(rig.eyes, { scaleY: 1, duration: 0.32 / speed, ease: m.ease.release }, '+=0.55')
                        .to(rig.cheeks, { scale: 1, duration: 0.4 / speed, ease: 'power2.out' }, '<');
                }),
            widen: () =>
                expression((rig, tl) => {
                    tl.to(rig.eyes, { scale: 1.2, duration: 0.1 / speed, ease: 'power2.out' }).to(
                        rig.eyes,
                        { scale: 1, duration: 0.5 / speed, ease: m.ease.release },
                        '+=0.7',
                    );
                }),
            dizzy: () =>
                expression((rig, tl) => {
                    tl.to(rig.eyes, { scaleY: 0.12, duration: 0.15 / speed, ease: 'power2.in' })
                        .to(rig.cheeks, { scale: 1.9, duration: 0.25 / speed, ease: m.ease.pop }, 0)
                        .to(
                            rig.eyes,
                            { rotate: 10, duration: 0.22 / speed, ease: 'sine.inOut', yoyo: true, repeat: 7 },
                            0.1,
                        )
                        .to(rig.eyes, { rotate: 0, scaleY: 1, duration: 0.4 / speed, ease: m.ease.release })
                        .to(rig.cheeks, { scale: 1, duration: 0.6 / speed, ease: 'power2.out' }, '<');
                }),
            doze: (on) => {
                const rig = rigRef.current;
                if (!rig || dozingRef.current === on) return;
                dozingRef.current = on;
                const eyes = [...rig.eyes, ...rig.heldCatEyes];
                exprRef.current?.kill();
                exprRef.current = null;
                if (on) {
                    blinksRef.current.forEach((tl) => tl.pause());
                    gsap.to(eyes, { scaleY: 0.08, duration: 0.6 / speed, ease: 'power2.inOut', ...EYE_ORIGIN });
                } else {
                    gsap.to(eyes, {
                        scaleY: 1,
                        duration: 0.25 / speed,
                        ease: 'power2.out',
                        onComplete: () => blinksRef.current.forEach((tl) => tl.play()),
                    });
                }
            },
        };
    }, [enabled, m.speed, m.ease]);
}
