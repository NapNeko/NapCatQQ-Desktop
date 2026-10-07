// 每帧消化剩余路程，行高测量和新消息改变终点时仍沿当前方向落到底部。
import { useCallback, useEffect, useRef, useState, type RefObject } from 'react';

export function useLatestScroll(
    scroll: RefObject<HTMLDivElement>,
    options: { enabled: boolean; duration: number; detach: () => void; finish: () => void },
) {
    const latest = useRef(options);
    latest.current = options;
    const frame = useRef(0);
    const request = useRef<AbortController | undefined>(undefined);
    const observer = useRef<ResizeObserver | undefined>(undefined);
    const active = useRef(false);
    const [running, setRunning] = useState(false);
    const cancel = useCallback(() => {
        request.current?.abort();
        request.current = undefined;
        active.current = false;
        cancelAnimationFrame(frame.current);
        frame.current = 0;
        observer.current?.disconnect();
        observer.current = undefined;
        setRunning(false);
    }, []);
    useEffect(() => {
        if (!options.enabled) cancel();
    }, [options.enabled, cancel]);
    useEffect(
        () => () => {
            request.current?.abort();
            active.current = false;
            cancelAnimationFrame(frame.current);
            frame.current = 0;
            observer.current?.disconnect();
        },
        [],
    );
    const start = useCallback(
        (load?: (signal: AbortSignal) => Promise<void>) => {
            cancel();
            const element = scroll.current;
            if (!element) return;
            const config = latest.current;
            if (!config.enabled && !load) {
                config.finish();
                return;
            }
            config.detach();
            const controller = new AbortController();
            request.current = controller;
            active.current = true;
            setRunning(true);
            let loaded = !load;
            const begin = () => {
                if (controller.signal.aborted) return;
                if (!element.isConnected) {
                    cancel();
                    return;
                }
                const duration = Math.min(300, Math.max(160, config.duration * 0.5));
                const started = performance.now();
                let previous = 0;
                let stable = 0;
                let previousEnd = -1;
                const tick = (now: number) => {
                    frame.current = 0;
                    if (controller.signal.aborted) return;
                    if (!element.isConnected) {
                        cancel();
                        return;
                    }
                    const progress = latest.current.enabled
                        ? Math.min(1, Math.max(0, (now - started) / duration))
                        : 1;
                    // 快速起步、连续减速；按尚未消化的比例重算，不覆盖锚点补偿。
                    const eased = 1 - Math.pow(1 - progress, 3);
                    const remaining = Math.max(
                        0,
                        element.scrollHeight - element.clientHeight - element.scrollTop,
                    );
                    if (progress === 1) {
                        const end = Math.max(0, element.scrollHeight - element.clientHeight);
                        element.scrollTop = end;
                        stable =
                            Math.abs(element.scrollTop - end) <= 1 && end === previousEnd
                                ? stable + 1
                                : 0;
                        previousEnd = end;
                        if (stable < 2) {
                            frame.current = requestAnimationFrame(tick);
                            return;
                        }
                        // 视口已到达时不空转等待网络；回包或行高改变再补齐终点。
                        if (!loaded) return;
                        frame.current = 0;
                        observer.current?.disconnect();
                        observer.current = undefined;
                        request.current = undefined;
                        active.current = false;
                        setRunning(false);
                        latest.current.finish();
                        return;
                    }
                    element.scrollTop +=
                        (remaining * (eased - previous)) / Math.max(0.000001, 1 - previous);
                    previous = eased;
                    frame.current = requestAnimationFrame(tick);
                };
                const schedule = () => {
                    if (!controller.signal.aborted && !frame.current && active.current)
                        frame.current = requestAnimationFrame(tick);
                };
                observer.current = new ResizeObserver(schedule);
                observer.current.observe(element);
                if (element.lastElementChild) observer.current.observe(element.lastElementChild);
                schedule();
                if (load) {
                    const settle = () => {
                        if (controller.signal.aborted) return;
                        loaded = true;
                        stable = 0;
                        schedule();
                    };
                    void load(controller.signal).then(settle, settle);
                }
            };
            begin();
        },
        [cancel, scroll],
    );
    const isRunning = useCallback(() => active.current, []);
    return { start, cancel, running, isRunning };
}
