// 每帧消化剩余路程，行高测量和新消息改变终点时仍沿当前方向落到底部。
import { useCallback, useEffect, useRef, useState, type RefObject } from 'react';

export function useLatestScroll(scroll: RefObject<HTMLDivElement>, options: { enabled: boolean; duration: number; detach: () => void; finish: () => void }) {
    const latest = useRef(options); latest.current = options;
    const frame = useRef(0);
    const [running, setRunning] = useState(false);
    const cancel = useCallback(() => { cancelAnimationFrame(frame.current); frame.current = 0; setRunning(false); }, []);
    useEffect(() => {
        if (!options.enabled) cancel();
        return () => { cancelAnimationFrame(frame.current); frame.current = 0; };
    }, [options.enabled, cancel]);
    const start = useCallback(() => {
        cancel();
        const element = scroll.current;
        if (!element) return;
        const config = latest.current;
        if (!config.enabled) { config.finish(); return; }
        config.detach();
        setRunning(true);
        const distance = Math.max(0, element.scrollHeight - element.clientHeight - element.scrollTop);
        const duration = Math.min(800, Math.max(280, config.duration * (1 + Math.log1p(distance / Math.max(1, element.clientHeight)) * .22)));
        const started = performance.now();
        let previous = 0;
        const tick = (now: number) => {
            if (!element.isConnected) { cancel(); return; }
            const progress = Math.min(1, Math.max(0, (now - started) / duration));
            // 快速起步、连续减速；按尚未消化的比例重算，不覆盖锚点补偿。
            const eased = 1 - Math.pow(1 - progress, 4);
            const remaining = Math.max(0, element.scrollHeight - element.clientHeight - element.scrollTop);
            if (progress === 1) {
                frame.current = 0; setRunning(false); latest.current.finish(); return;
            }
            element.scrollTop += remaining * (eased - previous) / Math.max(.000001, 1 - previous);
            previous = eased;
            frame.current = requestAnimationFrame(tick);
        };
        frame.current = requestAnimationFrame(tick);
    }, [cancel, scroll]);
    return { start, cancel, running };
}
