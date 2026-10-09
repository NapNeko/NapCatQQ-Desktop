import { useCallback, useEffect, useRef, type RefObject } from 'react';

// 鼠标滚轮分帧消化增量；触控板的细粒度惯性和触摸仍交给浏览器。
// 使用剩余增量而非绝对目标，历史 prepend 的锚点修正不会被下一帧覆盖。
export function useSmoothWheel(scroll: RefObject<HTMLDivElement>, enabled: boolean) {
    const stop = useRef(() => {});
    useEffect(() => {
        const element = scroll.current;
        if (!element || !enabled) return;
        let frame = 0;
        let remaining = 0;
        let last = 0;
        const cancel = () => {
            cancelAnimationFrame(frame);
            frame = 0;
            remaining = 0;
            last = 0;
        };
        stop.current = cancel;
        const advance = (step: number) => {
            const before = element.scrollTop;
            element.scrollTop += step;
            const moved = element.scrollTop - before;
            remaining -= moved;
            return Math.abs(remaining) >= 0.1 && Math.abs(moved) >= 0.01;
        };
        const tick = (now: number) => {
            const elapsed = Math.max(1, now - last);
            last = now;
            const step =
                Math.abs(remaining) < 1
                    ? remaining
                    : Math.sign(remaining) *
                      Math.max(1, Math.abs(remaining) * (1 - Math.exp(-elapsed / 32)));
            if (!advance(step)) cancel();
            else frame = requestAnimationFrame(tick);
        };
        const wheel = (event: WheelEvent) => {
            if (
                event.defaultPrevented ||
                !event.cancelable ||
                event.ctrlKey ||
                event.shiftKey ||
                event.deltaX ||
                !event.deltaY
            ) {
                cancel();
                return;
            }
            const discrete =
                event.deltaMode !== 0 ||
                (Math.abs(event.deltaY) >= 80 && Number.isInteger(event.deltaY));
            if (!discrete) {
                cancel();
                return;
            }
            const delta =
                event.deltaY *
                (event.deltaMode === 1 ? 20 : event.deltaMode === 2 ? element.clientHeight : 1);
            const end = Math.max(0, element.scrollHeight - element.clientHeight);
            if (
                !element.clientHeight ||
                (delta < 0 && element.scrollTop <= 0) ||
                (delta > 0 && element.scrollTop >= end)
            ) {
                cancel();
                return;
            }
            event.preventDefault();
            if (remaining && Math.sign(delta) !== Math.sign(remaining)) cancel();
            remaining = Math.max(
                -element.clientHeight * 2,
                Math.min(element.clientHeight * 2, remaining + delta),
            );
            // 输入先响应，短尾部再分帧消化；掉帧后按真实时间追上输入。
            if (
                !advance(remaining * 0.4) ||
                (remaining < 0 && element.scrollTop <= 0) ||
                (remaining > 0 && element.scrollTop >= end)
            ) {
                cancel();
                return;
            }
            if (!frame) {
                last = performance.now();
                frame = requestAnimationFrame(tick);
            }
        };
        element.addEventListener('wheel', wheel, { passive: false });
        element.addEventListener('pointerdown', cancel);
        element.addEventListener('keydown', cancel);
        element.addEventListener('touchstart', cancel, { passive: true });
        return () => {
            cancel();
            stop.current = () => {};
            element.removeEventListener('wheel', wheel);
            element.removeEventListener('pointerdown', cancel);
            element.removeEventListener('keydown', cancel);
            element.removeEventListener('touchstart', cancel);
        };
    }, [scroll, enabled]);
    return useCallback(() => stop.current(), []);
}
