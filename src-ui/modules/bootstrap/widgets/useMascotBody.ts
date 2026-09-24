// 给分层吉祥物装上身体动作：待机呼吸，戳一下随机来一段（蹦、歪头、点头、举猫），
// 打瞌睡时整个人塌下去。动作和呼吸都改 figure 的 scale，跑动作时先把呼吸停掉。
// 位移单位全是 viewBox 单位（1024 宽渲染成 150px 时 1 单位约 0.15px）。

import { useEffect, useMemo, useRef, type RefObject } from 'react';
import gsap from 'gsap';
import type { MotionEnv } from '../../../hooks/preferences/useMotion';
import { MASCOT_PIVOTS, type MascotLayer } from './mascotRig';

export type MascotMove = 'hop' | 'tilt' | 'nod' | 'liftCat';

export interface MascotBody {
    /** 随机挑一个动作，不和上一次重复。 */
    move: () => MascotMove | null;
    hop: () => void;
    tilt: (dir?: 1 | -1) => void;
    nod: () => void;
    liftCat: () => void;
    /** 塌下去打瞌睡 / 站直。 */
    slump: (on: boolean) => void;
}

const NOOP_BODY: MascotBody = {
    move: () => null,
    hop: () => { },
    tilt: () => { },
    nod: () => { },
    liftCat: () => { },
    slump: () => { },
};

type Parts = Record<MascotLayer | 'figure', SVGGElement | null>;

const MOVES: readonly MascotMove[] = ['hop', 'tilt', 'nod', 'liftCat'];

function queryParts(root: ParentNode): Parts {
    const q = (name: string) => root.querySelector<SVGGElement>(`[data-part="${name}"]`);
    return {
        figure: q('figure'),
        ground: q('ground'),
        body: q('body'),
        heldCat: q('heldCat'),
        head: q('head'),
        floorCat: q('floorCat'),
    };
}

const BREATH_CLASS = 'ndf-mascot-breathe';

/** 呼吸是 CSS 动画，开关就是加减类名；去掉再加回等于从头播，和原来 restart() 一样。 */
function setBreathing(el: HTMLElement | null, on: boolean): void {
    if (!el) return;
    el.classList.remove(BREATH_CLASS);
    if (on) {
        void el.getBoundingClientRect();
        el.classList.add(BREATH_CLASS);
    }
}

export function useMascotBody(bodyRef: RefObject<HTMLElement>, m: MotionEnv): MascotBody {
    const partsRef = useRef<Parts | null>(null);
    const breathingRef = useRef<HTMLElement | null>(null);
    const moveRef = useRef<gsap.core.Timeline | null>(null);
    const lastMoveRef = useRef<MascotMove | null>(null);
    const slumpedRef = useRef(false);

    const enabled = m.enabled;
    const lively = enabled && m.level !== 'elegant';

    useEffect(() => {
        const root = bodyRef.current;
        if (!root || !enabled) return;
        const parts = queryParts(root);
        if (!parts.figure || !parts.head) return;
        partsRef.current = parts;
        const all = Object.values(parts).filter((el): el is SVGGElement => el !== null);

        // 呼吸挂在包着 SVG 的这层 div 上（见 index.css .ndf-mascot-breathe）。
        // 原点是这个 div 自带的 50% 100%，正好是脚底，和原来的 svgOrigin: feet 一致。
        if (lively) {
            const speed = Math.max(0.5, m.speed);
            root.style.setProperty('--ndf-breath-dur', `${(2.6 / speed).toFixed(2)}s`);
            breathingRef.current = root;
            setBreathing(root, true);
        }

        return () => {
            setBreathing(root, false);
            breathingRef.current = null;
            moveRef.current?.kill();
            moveRef.current = null;
            gsap.killTweensOf(all);
            gsap.set(all, { clearProps: 'transform,opacity' });
            partsRef.current = null;
            slumpedRef.current = false;
        };
    }, [bodyRef, enabled, lively, m.speed]);

    return useMemo<MascotBody>(() => {
        if (!enabled) return NOOP_BODY;

        const speed = Math.max(0.5, m.speed);
        const s = (sec: number) => sec / speed;
        const resumeBreath = () => {
            if (!slumpedRef.current) setBreathing(breathingRef.current, true);
        };
        // 一次只跑一个动作，新动作来了先杀旧的。呼吸在外层 div 上，和这里改的 SVG 层
        // 互不干涉，跑动作时不用停它。
        const play = (build: (p: Parts, tl: gsap.core.Timeline) => void) => {
            const p = partsRef.current;
            if (!p || !p.figure || !p.head || slumpedRef.current) return;
            moveRef.current?.kill();
            const tl = gsap.timeline();
            moveRef.current = tl;
            build(p, tl);
        };

        // 蹦一下：蓄力下蹲 → 起跳拉长 → 落地压扁 → 弹回。时间点都是绝对值（已按 speed 缩放），
        // 头和怀里的猫比身体慢半拍（起跳被甩下、落地再沉一下），影子随高度缩小，地上的猫晚半拍跟着蹦。
        const hop = () =>
            play((p, tl) => {
                tl.to(p.figure, { scaleY: 0.94, scaleX: 1.03, duration: s(0.14), ease: 'power2.out', svgOrigin: MASCOT_PIVOTS.feet }, 0)
                    .to(p.figure, { y: -70, scaleY: 1.03, scaleX: 0.985, duration: s(0.28), ease: 'power2.out' }, s(0.14))
                    .to(p.figure, { y: 0, scaleY: 0.95, scaleX: 1.04, duration: s(0.26), ease: 'power2.in' }, s(0.42))
                    .to(p.figure, { scaleY: 1, scaleX: 1, duration: s(0.55), ease: m.ease.release }, s(0.68))
                    .to(p.head, { y: 10, duration: s(0.14), ease: 'power1.out' }, s(0.14))
                    .to(p.head, { y: -6, duration: s(0.2), ease: 'power1.inOut' }, s(0.28))
                    .to(p.head, { y: 7, duration: s(0.12), ease: 'power1.out' }, s(0.62))
                    .to(p.head, { y: 0, duration: s(0.32), ease: m.ease.release }, s(0.74));
                if (p.heldCat) {
                    tl.to(p.heldCat, { y: 12, duration: s(0.14), ease: 'power1.out' }, s(0.17))
                        .to(p.heldCat, { y: -8, duration: s(0.2), ease: 'power1.inOut' }, s(0.31))
                        .to(p.heldCat, { y: 8, duration: s(0.12), ease: 'power1.out' }, s(0.65))
                        .to(p.heldCat, { y: 0, duration: s(0.32), ease: m.ease.release }, s(0.77));
                }
                if (p.ground) {
                    tl.to(
                        p.ground,
                        { scaleX: 0.86, scaleY: 0.8, opacity: 0.6, duration: s(0.28), ease: 'power2.out', svgOrigin: MASCOT_PIVOTS.shadow },
                        s(0.14),
                    ).to(p.ground, { scaleX: 1, scaleY: 1, opacity: 1, duration: s(0.26), ease: 'power2.in' }, s(0.42));
                }
                if (p.floorCat) {
                    tl.to(
                        p.floorCat,
                        { y: -34, scaleY: 1.06, duration: s(0.22), ease: 'power2.out', svgOrigin: MASCOT_PIVOTS.floorCat },
                        s(0.5),
                    )
                        .to(p.floorCat, { y: 0, scaleY: 0.95, duration: s(0.2), ease: 'power2.in' }, s(0.72))
                        .to(p.floorCat, { scaleY: 1, duration: s(0.4), ease: m.ease.release }, s(0.92));
                }
            });

        const tilt = (dir: 1 | -1 = Math.random() < 0.5 ? -1 : 1) =>
            play((p, tl) => {
                const hold = s(0.32) + 0.7;
                tl.to(p.head, { rotation: 5 * dir, duration: s(0.32), ease: m.ease.pop, svgOrigin: MASCOT_PIVOTS.neck }, 0).to(
                    p.head,
                    { rotation: 0, duration: s(0.45), ease: m.ease.release },
                    hold,
                );
                if (p.heldCat) {
                    tl.to(p.heldCat, { y: -4, duration: s(0.32), ease: m.ease.pop }, 0).to(
                        p.heldCat,
                        { y: 0, duration: s(0.45), ease: m.ease.release },
                        hold,
                    );
                }
            });

        const nod = () =>
            play((p, tl) => {
                tl.to(p.head, {
                    y: 9,
                    scaleY: 0.96,
                    duration: s(0.15),
                    ease: 'power2.inOut',
                    yoyo: true,
                    repeat: 3,
                    svgOrigin: MASCOT_PIVOTS.neck,
                }).to(p.figure, { scaleY: 0.99, duration: s(0.3), ease: 'sine.inOut', yoyo: true, repeat: 1, svgOrigin: MASCOT_PIVOTS.feet }, 0);
            });

        // 举猫：猫抬高一点，她低头看猫。没有怀里猫的层就退化成点头。
        const liftCat = () =>
            play((p, tl) => {
                if (!p.heldCat) {
                    nod();
                    return;
                }
                const hold = s(0.35) + 0.75;
                tl.to(p.heldCat, { y: -16, rotation: -4, duration: s(0.3), ease: m.ease.pop, svgOrigin: MASCOT_PIVOTS.heldCat }, 0)
                    .to(p.head, { rotation: -4, y: 3, duration: s(0.3), ease: m.ease.pop, svgOrigin: MASCOT_PIVOTS.neck }, s(0.05))
                    .to(p.heldCat, { y: 0, rotation: 0, duration: s(0.5), ease: m.ease.release }, hold)
                    .to(p.head, { rotation: 0, y: 0, duration: s(0.5), ease: m.ease.release }, hold);
            });

        const table: Record<MascotMove, () => void> = { hop, tilt: () => tilt(), nod, liftCat };
        // elegant 档只留歪头和点头，蹦跳和举猫太闹。
        const moves = lively ? MOVES : MOVES.filter((k) => k === 'tilt' || k === 'nod');

        return {
            move: () => {
                if (!partsRef.current || slumpedRef.current) return null;
                const pool = moves.filter((k) => k !== lastMoveRef.current);
                const pick = pool[Math.floor(Math.random() * pool.length)];
                lastMoveRef.current = pick;
                table[pick]();
                return pick;
            },
            hop,
            tilt,
            nod,
            liftCat,
            slump: (on) => {
                const p = partsRef.current;
                if (!p || !p.figure || !p.head || slumpedRef.current === on) return;
                slumpedRef.current = on;
                moveRef.current?.kill();
                moveRef.current = null;
                setBreathing(breathingRef.current, false);
                if (on) {
                    // 顺时针转：左边（头发贴着猫耳那侧）往上抬，不会被窗口切到；垂头感靠 y 给
                    gsap.to(p.head, { rotation: 4, y: 9, duration: s(0.9), ease: 'power2.inOut', svgOrigin: MASCOT_PIVOTS.neck });
                    gsap.to(p.figure, { scaleY: 0.985, duration: s(0.9), ease: 'power2.inOut', svgOrigin: MASCOT_PIVOTS.feet });
                    if (p.heldCat) gsap.to(p.heldCat, { y: 5, duration: s(0.9), ease: 'power2.inOut' });
                } else {
                    gsap.to(p.head, { rotation: 0, y: 0, duration: s(0.3), ease: m.ease.release, svgOrigin: MASCOT_PIVOTS.neck });
                    gsap.to(p.figure, {
                        scaleY: 1,
                        duration: s(0.3),
                        ease: m.ease.release,
                        svgOrigin: MASCOT_PIVOTS.feet,
                        onComplete: resumeBreath,
                    });
                    if (p.heldCat) gsap.to(p.heldCat, { y: 0, duration: s(0.3), ease: m.ease.release });
                }
            },
        };
    }, [enabled, lively, m.speed, m.ease]);
}
