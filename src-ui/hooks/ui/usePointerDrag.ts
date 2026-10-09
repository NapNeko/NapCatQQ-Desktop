// 拖动只保留最新坐标，松手应用最后一帧；中断时统一回滚并释放捕获。
import {
    useCallback,
    useLayoutEffect,
    useRef,
    useState,
    type PointerEvent as ReactPointerEvent,
} from 'react';

type DragEnd = 'commit' | 'cancel' | 'unmount';

interface PointerDragOptions<T extends HTMLElement> {
    cursor: 'col-resize' | 'row-resize';
    onStart: (event: ReactPointerEvent<T>) => boolean | void;
    onMove: (event: PointerEvent) => void;
    onEnd: (reason: DragEnd) => void;
}

export function usePointerDrag<T extends HTMLElement>(options: PointerDragOptions<T>) {
    const optionsRef = useRef(options);
    optionsRef.current = options;
    const active = useRef<{ finish: (reason: DragEnd) => void } | null>(null);
    const [dragging, setDragging] = useState(false);

    useLayoutEffect(() => () => active.current?.finish('unmount'), []);
    const cancel = useCallback(() => active.current?.finish('cancel'), []);
    const start = useCallback((event: ReactPointerEvent<T>) => {
        if (event.button !== 0 || event.isPrimary === false) return;
        active.current?.finish('cancel');
        const callbacks = optionsRef.current;
        if (callbacks.onStart(event) === false) return;
        event.preventDefault();
        const target = event.currentTarget;
        const document = target.ownerDocument;
        const win = document.defaultView ?? window;
        const body = document.body;
        const pointerId = event.pointerId;
        const saved = ['cursor', 'user-select'].map((name) => ({
            name,
            value: body.style.getPropertyValue(name),
            priority: body.style.getPropertyPriority(name),
        }));
        body.style.setProperty('cursor', callbacks.cursor, 'important');
        body.style.setProperty('user-select', 'none', 'important');
        let frame: number | null = null;
        let latest: PointerEvent | null = null;
        let moved = false;
        const flush = () => {
            frame = null;
            const next = latest;
            latest = null;
            if (next) callbacks.onMove(next);
        };
        const move = (next: PointerEvent) => {
            if (next.pointerId !== pointerId) return;
            moved ||= next.clientX !== event.clientX || next.clientY !== event.clientY;
            latest = next;
            if (frame === null) frame = win.requestAnimationFrame(flush);
        };
        const up = (next: PointerEvent) => {
            if (next.pointerId !== pointerId) return;
            const changed =
                moved || next.clientX !== event.clientX || next.clientY !== event.clientY;
            if (changed) latest = next;
            session.finish(changed ? 'commit' : 'cancel');
        };
        const interrupted = (next: PointerEvent) => {
            if (next.pointerId === pointerId) session.finish('cancel');
        };
        const blur = () => session.finish('cancel');
        const hidden = () => {
            if (document.hidden) session.finish('cancel');
        };
        const session = {
            finish(reason: DragEnd) {
                if (active.current !== session) return;
                if (frame !== null) win.cancelAnimationFrame(frame);
                frame = null;
                if (reason === 'commit') flush();
                latest = null;
                active.current = null;
                win.removeEventListener('pointermove', move, true);
                win.removeEventListener('pointerup', up, true);
                win.removeEventListener('pointercancel', interrupted, true);
                win.removeEventListener('blur', blur);
                win.removeEventListener('resize', blur);
                document.removeEventListener('visibilitychange', hidden);
                target.removeEventListener('lostpointercapture', interrupted);
                for (const { name, value, priority } of saved) {
                    if (value) body.style.setProperty(name, value, priority);
                    else body.style.removeProperty(name);
                }
                try {
                    if (target.hasPointerCapture(pointerId))
                        target.releasePointerCapture(pointerId);
                } catch {
                    // 窗口销毁或指针已取消时，捕获可能已经由平台释放。
                }
                if (reason !== 'unmount') setDragging(false);
                callbacks.onEnd(reason);
            },
        };
        active.current = session;
        win.addEventListener('pointermove', move, true);
        win.addEventListener('pointerup', up, true);
        win.addEventListener('pointercancel', interrupted, true);
        win.addEventListener('blur', blur);
        win.addEventListener('resize', blur);
        document.addEventListener('visibilitychange', hidden);
        target.addEventListener('lostpointercapture', interrupted);
        try {
            target.setPointerCapture(pointerId);
        } catch {
            // 捕获不可用时仍由窗口监听收尾，不让拖动状态和全局样式残留。
        }
        setDragging(true);
    }, []);

    return { dragging, start, cancel };
}
