// 分隔线的宽度受聊天画布约束，拖动结束才保存偏好。
import { useEffect, useRef, useState } from 'react';
import { usePointerDrag } from '../../hooks/ui/usePointerDrag';

export function ChatDivider({
    width,
    onResize,
    onCommit,
}: {
    width: number;
    onResize: (width: number) => void;
    onCommit: (width: number) => void;
}) {
    const element = useRef<HTMLButtonElement>(null);
    const drag = useRef<{
        x: number;
        width: number;
        current: number;
        max: number;
        workspace: HTMLElement;
        original: string;
    } | null>(null);
    const widthRef = useRef(width);
    widthRef.current = width;
    const [bounds, setBounds] = useState({ current: width, max: 380 });
    useEffect(() => {
        const pane = element.current?.parentElement;
        const workspace = pane?.parentElement;
        if (!pane || !workspace) return;
        const measure = () => {
            if (drag.current) return;
            const current = Math.round(pane.getBoundingClientRect().width) || widthRef.current;
            const max = Math.round(Math.min(380, Math.max(220, workspace.clientWidth * 0.42)));
            setBounds((previous) =>
                previous.current === current && previous.max === max ? previous : { current, max },
            );
        };
        const observer = new ResizeObserver(measure);
        observer.observe(pane);
        observer.observe(workspace);
        measure();
        return () => observer.disconnect();
    }, []);
    const clamp = (value: number) => Math.round(Math.min(bounds.max, Math.max(220, value)));
    const commit = (value: number) => {
        const next = clamp(value);
        onResize(next);
        onCommit(next);
    };
    const resize = usePointerDrag<HTMLButtonElement>({
        cursor: 'col-resize',
        onStart: (event) => {
            const pane = event.currentTarget.parentElement;
            const workspace = pane?.parentElement;
            if (!pane || !workspace) return false;
            event.currentTarget.focus();
            const current = Math.round(pane.getBoundingClientRect().width);
            drag.current = {
                x: event.clientX,
                width: current,
                current,
                max: Math.round(Math.min(380, Math.max(220, workspace.clientWidth * 0.42))),
                workspace,
                original: workspace.style.getPropertyValue('--chat-list-width'),
            };
            return true;
        },
        onMove: (event) => {
            const current = drag.current;
            if (!current) return;
            const next = Math.round(
                Math.min(current.max, Math.max(220, current.width + event.clientX - current.x)),
            );
            if (next === current.current) return;
            current.current = next;
            current.workspace.style.setProperty('--chat-list-width', `${next}px`);
            element.current?.setAttribute('aria-valuenow', String(next));
        },
        onEnd: (reason) => {
            const current = drag.current;
            drag.current = null;
            if (!current) return;
            if (reason === 'commit') {
                onResize(current.current);
                onCommit(current.current);
            } else {
                if (current.original)
                    current.workspace.style.setProperty('--chat-list-width', current.original);
                else current.workspace.style.removeProperty('--chat-list-width');
                element.current?.setAttribute('aria-valuenow', String(current.width));
            }
            if (reason !== 'unmount')
                setBounds({
                    current: reason === 'commit' ? current.current : current.width,
                    max: current.max,
                });
        },
    });
    return (
        <button
            ref={element}
            role="separator"
            className="native-chat-divider"
            aria-label="调整会话列表宽度"
            aria-orientation="vertical"
            aria-valuemin={220}
            aria-valuemax={bounds.max}
            aria-valuenow={bounds.current}
            title="拖动调整宽度，双击恢复"
            onDoubleClick={() => commit(260)}
            onKeyDown={(e) => {
                const next =
                    e.key === 'ArrowLeft'
                        ? bounds.current - 10
                        : e.key === 'ArrowRight'
                          ? bounds.current + 10
                          : e.key === 'Home'
                            ? 220
                            : e.key === 'End'
                              ? bounds.max
                              : e.key === 'Enter'
                                ? 260
                                : null;
                if (next !== null) {
                    e.preventDefault();
                    commit(next);
                }
            }}
            onPointerDown={resize.start}
        />
    );
}
