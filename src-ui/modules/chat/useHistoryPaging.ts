// 只响应向上阅读的意图，加载和自动锚点调整不会连续触发请求。
import { useMemo, useRef, type RefObject } from 'react';

export function useHistoryPaging(options: { scroll: RefObject<HTMLDivElement>; enabled: boolean; load: () => Promise<void>; detach: () => void }) {
    const current = useRef(options); current.current = options;
    const busy = useRef(false); const armed = useRef(false); const lastTop = useRef(0); const touch = useRef<number | null>(null);
    return useMemo(() => {
        const loadNearTop = () => {
            const { scroll, enabled, load, detach } = current.current;
            if (!armed.current || !enabled || busy.current || !scroll.current || scroll.current.scrollTop > 160) return;
            armed.current = false; busy.current = true; detach();
            void load().then(() => { busy.current = false; }, () => { busy.current = false; });
        };
        return {
            onScroll: () => {
                const top = current.current.scroll.current?.scrollTop ?? 0;
                if (top <= lastTop.current) loadNearTop();
                lastTop.current = top;
            },
            onWheel: (event: { deltaY: number }) => {
                armed.current = event.deltaY < 0;
                lastTop.current = current.current.scroll.current?.scrollTop ?? 0;
                if (armed.current) loadNearTop();
            },
            onKeyDown: (event: { key: string }) => {
                armed.current = ['ArrowUp', 'PageUp', 'Home'].includes(event.key);
                lastTop.current = current.current.scroll.current?.scrollTop ?? 0;
                if (armed.current) loadNearTop();
            },
            onPointerDown: () => { armed.current = true; lastTop.current = current.current.scroll.current?.scrollTop ?? 0; },
            onTouchStart: (event: { touches: ArrayLike<{ clientY: number }> }) => { touch.current = event.touches[0]?.clientY ?? null; },
            onTouchMove: (event: { touches: ArrayLike<{ clientY: number }> }) => {
                const y = event.touches[0]?.clientY;
                armed.current = y !== undefined && touch.current !== null && y > touch.current;
                touch.current = y ?? null;
                if (armed.current) loadNearTop();
            },
        };
    }, []);
}
