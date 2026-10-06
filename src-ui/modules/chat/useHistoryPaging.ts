// 只响应向上阅读的意图，加载和自动锚点调整不会连续触发请求。
import { useEffect, useMemo, useRef, type RefObject } from 'react';

const PREFETCH_PX = 240;

export function useHistoryPaging(options: {
    scroll: RefObject<HTMLDivElement>;
    enabled: boolean;
    load: () => Promise<void>;
    detach: () => void;
}) {
    const current = useRef(options);
    current.current = options;
    const busy = useRef(false);
    const armed = useRef(false);
    const touch = useRef<number | null>(null);
    const frame = useRef<number | null>(null);
    useEffect(
        () => () => {
            if (frame.current !== null) cancelAnimationFrame(frame.current);
        },
        [],
    );
    return useMemo(() => {
        const loadNearTop = () => {
            const { scroll, enabled, load, detach } = current.current;
            if (
                !armed.current ||
                !enabled ||
                busy.current ||
                !scroll.current ||
                scroll.current.scrollTop > PREFETCH_PX
            )
                return;
            armed.current = false;
            busy.current = true;
            detach();
            void load().then(
                () => {
                    busy.current = false;
                },
                () => {
                    busy.current = false;
                },
            );
        };
        const scheduleLoad = () => {
            if (frame.current !== null) return;
            frame.current = requestAnimationFrame(() => {
                frame.current = requestAnimationFrame(() => {
                    frame.current = null;
                    loadNearTop();
                });
            });
        };
        return {
            onScroll: () => {
                if (
                    armed.current &&
                    (current.current.scroll.current?.scrollTop ?? Infinity) <= PREFETCH_PX
                )
                    scheduleLoad();
            },
            onWheel: (event: { deltaY: number }) => {
                armed.current = event.deltaY < 0;
                if (
                    armed.current &&
                    (current.current.scroll.current?.scrollTop ?? Infinity) <= PREFETCH_PX
                )
                    scheduleLoad();
            },
            onKeyDown: (event: { key: string }) => {
                armed.current = ['ArrowUp', 'PageUp', 'Home'].includes(event.key);
                if (
                    armed.current &&
                    (current.current.scroll.current?.scrollTop ?? Infinity) <= PREFETCH_PX
                )
                    scheduleLoad();
            },
            onPointerDown: () => {
                armed.current = true;
            },
            onTouchStart: (event: { touches: ArrayLike<{ clientY: number }> }) => {
                touch.current = event.touches[0]?.clientY ?? null;
            },
            onTouchMove: (event: { touches: ArrayLike<{ clientY: number }> }) => {
                const y = event.touches[0]?.clientY;
                armed.current = y !== undefined && touch.current !== null && y > touch.current;
                touch.current = y ?? null;
                if (
                    armed.current &&
                    (current.current.scroll.current?.scrollTop ?? Infinity) <= PREFETCH_PX
                )
                    scheduleLoad();
            },
        };
    }, []);
}
