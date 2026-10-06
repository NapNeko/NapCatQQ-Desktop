// 手动高度保留为界面偏好，窗口缩小时只限制显示高度。
import { useLayoutEffect, useRef, useState, type ComponentProps, type RefObject } from 'react';
import { setChatPreferences, useChatPreferences } from './chatPreferences';

export function useComposerResize(
    input: RefObject<HTMLTextAreaElement>,
    composer: RefObject<HTMLDivElement>,
    text: string,
) {
    const preference = useChatPreferences().composerHeight;
    const [manual, setManual] = useState(preference);
    const [bounds, setBounds] = useState({ height: 52, max: 420 });
    const drag = useRef<{
        y: number;
        height: number;
        original: number | null;
        current: number;
    } | null>(null);
    const measure = useRef(() => {});
    const clamp = (value: number) => Math.round(Math.max(52, Math.min(bounds.max, value)));
    useLayoutEffect(() => {
        setManual(preference);
    }, [preference]);
    useLayoutEffect(() => {
        measure.current = () => {
            const element = input.current;
            const container = composer.current;
            if (!element || !container) return;
            const pane = container.parentElement;
            const overhead = container.offsetHeight - element.offsetHeight;
            const available = pane?.clientHeight || window.innerHeight;
            const max = Math.round(
                Math.max(52, Math.min(420, available * 0.55, available - overhead - 180)),
            );
            let height: number;
            if (manual === null) {
                element.style.height = '0px';
                height = Math.min(max, 140, Math.max(52, element.scrollHeight));
            } else height = Math.min(max, Math.max(52, manual));
            element.style.height = `${height}px`;
            setBounds((previous) =>
                previous.height === height && previous.max === max ? previous : { height, max },
            );
        };
        measure.current();
    }, [text, manual, input, composer]);
    useLayoutEffect(() => {
        const element = input.current;
        const container = composer.current;
        const pane = container?.parentElement;
        if (!element || !container || !pane) return;
        let width = element.clientWidth;
        let paneHeight = pane.clientHeight;
        let overhead = container.offsetHeight - element.offsetHeight;
        const observer = new ResizeObserver(() => {
            const nextOverhead = container.offsetHeight - element.offsetHeight;
            if (
                width === element.clientWidth &&
                paneHeight === pane.clientHeight &&
                overhead === nextOverhead
            )
                return;
            width = element.clientWidth;
            paneHeight = pane.clientHeight;
            overhead = nextOverhead;
            measure.current();
        });
        observer.observe(pane);
        observer.observe(container);
        observer.observe(element);
        return () => observer.disconnect();
    }, [input, composer]);
    const commit = (height: number | null) => {
        setManual(height);
        setChatPreferences({ composerHeight: height });
    };
    return {
        type: 'button',
        role: 'separator',
        className: 'native-chat-composer-resize',
        'aria-label': '调整输入框高度',
        'aria-orientation': 'horizontal',
        'aria-valuemin': 52,
        'aria-valuemax': bounds.max,
        'aria-valuenow': bounds.height,
        title: '拖动调整高度，双击恢复自动高度',
        onDoubleClick: () => commit(null),
        onKeyDown: (event) => {
            if (event.key === 'Enter') {
                event.preventDefault();
                commit(null);
                return;
            }
            const next =
                event.key === 'ArrowUp'
                    ? bounds.height + 16
                    : event.key === 'ArrowDown'
                      ? bounds.height - 16
                      : event.key === 'Home'
                        ? 52
                        : event.key === 'End'
                          ? bounds.max
                          : null;
            if (next !== null) {
                event.preventDefault();
                commit(clamp(next));
            }
        },
        onPointerDown: (event) => {
            if (event.button !== 0) return;
            event.preventDefault();
            event.currentTarget.focus();
            event.currentTarget.setPointerCapture(event.pointerId);
            drag.current = {
                y: event.clientY,
                height: bounds.height,
                current: bounds.height,
                original: manual,
            };
        },
        onPointerMove: (event) => {
            if (!drag.current) return;
            drag.current.current = clamp(drag.current.height + drag.current.y - event.clientY);
            setManual(drag.current.current);
        },
        onPointerUp: (event) => {
            if (!drag.current) return;
            commit(drag.current.current);
            drag.current = null;
            event.currentTarget.releasePointerCapture(event.pointerId);
        },
        onPointerCancel: () => {
            if (drag.current) setManual(drag.current.original);
            drag.current = null;
        },
        onLostPointerCapture: () => {
            if (drag.current) setManual(drag.current.original);
            drag.current = null;
        },
    } satisfies ComponentProps<'button'>;
}
