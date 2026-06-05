// 列表进场 stagger：只对「尚未标记」的子节点跑 gsap.from，避免 bots.length /
// containers.length 等依赖变化时让整张列表重播一遍（轮询/事件更新 state 时
// 子节点数量常不变，旧写法会反复 gsap.from(all children) 造成卡顿）。

import gsap from 'gsap';
import type { MotionEnv } from '../../../hooks/preferences/useMotion';

const DATA_ENTERED = 'data-motion-entered';

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

    const useStagger = pending.length > 1 && m.stagger() > 0;

    gsap.from(pending, {
        autoAlpha: 0,
        y: 6,
        scale: 0.985,
        duration: m.duration('base'),
        ease: m.ease.enter,
        stagger: useStagger ? m.stagger() : 0,
        onComplete: () => {
            pending.forEach((el) => el.setAttribute(DATA_ENTERED, '1'));
        },
    });
}