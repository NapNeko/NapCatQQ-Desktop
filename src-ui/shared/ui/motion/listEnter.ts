// 列表进场 stagger：只对「尚未标记」的子节点跑动画，避免 bots.length /
// containers.length 等依赖变化时让整张列表重播一遍（轮询/事件更新 state 时
// 子节点数量常不变，旧写法会反复 gsap.from(all children) 造成卡顿）。
//
// 使用 fromTo + onComplete/onInterrupt 强制落终态：路由 PageTransition 与列表
// stagger 同帧时，gsap.from 被 kill 后首张卡可能卡在 autoAlpha:0 呈「发灰」。

import gsap from 'gsap';
import type { MotionEnv } from '../../../hooks/preferences/useMotion';

const DATA_ENTERED = 'data-motion-entered';

function markEntered(els: readonly HTMLElement[]): void {
    for (const el of els) {
        el.setAttribute(DATA_ENTERED, '1');
        gsap.set(el, { autoAlpha: 1, opacity: 1, visibility: 'visible', y: 0, scale: 1 });
    }
}

export function animateListChildrenEnter(
    container: HTMLElement,
    listLength: number,
    m: MotionEnv,
): void {
    if (!m.enabled || listLength === 0) return;

    const children = Array.from(container.children) as HTMLElement[];
    if (children.length === 0) return;

    const pending = children.filter((el) => el.getAttribute(DATA_ENTERED) !== '1');
    if (pending.length === 0) return;

    for (const el of pending) {
        gsap.killTweensOf(el);
    }

    const useStagger = pending.length > 1 && m.stagger() > 0;

    gsap.fromTo(
        pending,
        { autoAlpha: 0, y: 6, scale: 0.985, force3D: true },
        {
            autoAlpha: 1,
            y: 0,
            scale: 1,
            duration: m.duration('base'),
            ease: m.ease.enter,
            stagger: useStagger ? m.stagger() : 0,
            force3D: true,
            onComplete: () => markEntered(pending),
            onInterrupt: () => markEntered(pending),
        },
    );
}

/// 等当前帧 + 下一帧再跑列表进场，避免与 PageTransition 的 enter 抢同一 reflow。
export function animateListChildrenEnterAfterPaint(
    container: HTMLElement,
    listLength: number,
    m: MotionEnv,
): () => void {
    let innerId = 0;
    const outerId = requestAnimationFrame(() => {
        innerId = requestAnimationFrame(() => {
            animateListChildrenEnter(container, listLength, m);
        });
    });
    return () => {
        cancelAnimationFrame(outerId);
        if (innerId) cancelAnimationFrame(innerId);
    };
}