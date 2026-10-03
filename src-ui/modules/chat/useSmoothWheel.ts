import { useEffect, type RefObject } from 'react';

// 鼠标滚轮分帧消化增量；触控板的细粒度惯性和触摸仍交给浏览器。
// 使用剩余增量而非绝对目标，历史 prepend 的锚点修正不会被下一帧覆盖。
export function useSmoothWheel(scroll: RefObject<HTMLDivElement>, enabled: boolean) {
    useEffect(() => {
        const element = scroll.current;
        if (!element || !enabled) return;
        let frame = 0; let remaining = 0; let last = 0;
        const cancel = () => { cancelAnimationFrame(frame); frame = 0; remaining = 0; last = 0; };
        const tick = (now: number) => {
            const elapsed = last ? Math.min(32, now - last) : 16;
            last = now;
            const step = Math.abs(remaining) < .5 ? remaining : remaining * (1 - Math.exp(-elapsed / 55));
            const before = element.scrollTop;
            element.scrollTop += step;
            remaining -= step;
            if (Math.abs(remaining) < .1 || Math.abs(element.scrollTop - before) < .01) cancel();
            else frame = requestAnimationFrame(tick);
        };
        const wheel = (event: WheelEvent) => {
            if (event.ctrlKey || event.shiftKey || event.deltaX || !event.deltaY) return;
            const discrete = event.deltaMode !== 0 || Math.abs(event.deltaY) >= 40 && Number.isInteger(event.deltaY);
            if (!discrete) { cancel(); return; }
            event.preventDefault();
            const delta = event.deltaY * (event.deltaMode === 1 ? 20 : event.deltaMode === 2 ? element.clientHeight : 1);
            if (Math.sign(delta) !== Math.sign(remaining)) remaining = 0;
            remaining = Math.max(-element.clientHeight * 2, Math.min(element.clientHeight * 2, remaining + delta));
            if (!frame) frame = requestAnimationFrame(tick);
        };
        element.addEventListener('wheel', wheel, { passive: false });
        element.addEventListener('pointerdown', cancel);
        element.addEventListener('keydown', cancel);
        element.addEventListener('touchstart', cancel, { passive: true });
        return () => {
            cancel(); element.removeEventListener('wheel', wheel);
            element.removeEventListener('pointerdown', cancel);
            element.removeEventListener('keydown', cancel);
            element.removeEventListener('touchstart', cancel);
        };
    }, [scroll, enabled]);
}
