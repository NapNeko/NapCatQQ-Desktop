// 列表里的行因为展开 / 收起 / 拖动换位而移动时，从旧位置平滑滑到新位置（FLIP）。
//
// 用法：改状态之前调 capture()，记下所有 `[data-flip]` 元素的位置；等 `version`（列表数据本身）
// 变了的那次渲染完成后，按新旧位置差给每个元素播一段 transform 回到 0，新出现的元素淡入。
// 只动 transform / opacity：行本身已经在新位置上，动画只是视觉过渡，被打断也不会留下错位。
// 虚拟列表的外层行自己用 transform 定位，所以 data-flip 要挂在行里面那一层上。
//
// 为什么等 version 而不是等下一次渲染：收藏的改动先经过 mutation（isPending 变了会先重渲一次），
// 缓存里的数据下一个微任务才换，只认「数据变了」那一次才对得上。

import { useCallback, useLayoutEffect, useRef, type RefObject } from 'react';
import { useMotion } from '../../../hooks/preferences/useMotion';
import { cssEase } from '../../../core/design/cssEase';

/** 位移太大时（比如整段收起后跳了几屏）不播，免得一行从很远的地方飞过来 */
const MAX_TRAVEL_PX = 480;
/** 记下位置后这么久数据还没变，就当这次改动没发生（比如保存被拒），作废快照 */
const SNAPSHOT_TTL_MS = 1000;

export function useFlip(containerRef: RefObject<HTMLElement | null>, version: unknown): () => void {
    const m = useMotion();
    const snapshot = useRef<{ at: number; tops: Map<string, number> } | null>(null);

    const capture = useCallback(() => {
        const root = containerRef.current;
        if (!root || !m.enabled) return;
        const tops = new Map<string, number>();
        root.querySelectorAll<HTMLElement>('[data-flip]').forEach((el) => {
            const key = el.dataset.flip;
            if (key) tops.set(key, el.getBoundingClientRect().top);
        });
        snapshot.current = { at: performance.now(), tops };
    }, [containerRef, m.enabled]);

    useLayoutEffect(() => {
        const snap = snapshot.current;
        const root = containerRef.current;
        snapshot.current = null;
        if (!snap || !root || !m.enabled || performance.now() - snap.at > SNAPSHOT_TTL_MS) return;
        const duration = m.duration('fast') * 1000;
        const easing = cssEase(m.ease.enter);
        root.querySelectorAll<HTMLElement>('[data-flip]').forEach((el) => {
            const key = el.dataset.flip;
            if (!key || typeof el.animate !== 'function') return;
            const top = el.getBoundingClientRect().top;
            const old = snap.tops.get(key);
            if (old === undefined) {
                el.animate([{ opacity: 0 }, { opacity: 1 }], { duration, easing });
                return;
            }
            const dy = old - top;
            if (Math.abs(dy) < 0.5 || Math.abs(dy) > MAX_TRAVEL_PX) return;
            el.animate([{ transform: `translateY(${dy}px)` }, { transform: 'none' }], { duration, easing });
        });
        // 只在列表数据换了的那次渲染后播
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [version]);

    return capture;
}
