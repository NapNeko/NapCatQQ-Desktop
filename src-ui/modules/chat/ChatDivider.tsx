// 分隔线的宽度受聊天画布约束，拖动结束才保存偏好。
import { useEffect, useRef, useState } from 'react';

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
    const drag = useRef<{ x: number; width: number; current: number } | null>(null);
    const [bounds, setBounds] = useState({ current: width, max: 380 });
    useEffect(() => {
        const pane = element.current?.parentElement;
        const workspace = pane?.parentElement;
        if (!pane || !workspace) return;
        const measure = () =>
            setBounds({
                current: Math.round(pane.getBoundingClientRect().width) || width,
                max: Math.round(Math.min(380, Math.max(220, workspace.clientWidth * 0.42))),
            });
        const observer = new ResizeObserver(measure);
        observer.observe(pane);
        observer.observe(workspace);
        measure();
        return () => observer.disconnect();
    }, [width]);
    const clamp = (value: number) => Math.round(Math.min(bounds.max, Math.max(220, value)));
    const commit = (value: number) => {
        const next = clamp(value);
        onResize(next);
        onCommit(next);
    };
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
            onPointerDown={(e) => {
                if (e.button !== 0) return;
                e.preventDefault();
                e.currentTarget.focus();
                e.currentTarget.setPointerCapture(e.pointerId);
                drag.current = { x: e.clientX, width: bounds.current, current: bounds.current };
            }}
            onPointerMove={(e) => {
                if (!drag.current) return;
                drag.current.current = clamp(drag.current.width + e.clientX - drag.current.x);
                onResize(drag.current.current);
            }}
            onPointerUp={(e) => {
                if (!drag.current) return;
                onCommit(drag.current.current);
                drag.current = null;
                e.currentTarget.releasePointerCapture(e.pointerId);
            }}
            onPointerCancel={() => {
                if (drag.current) onResize(drag.current.width);
                drag.current = null;
            }}
            onLostPointerCapture={() => {
                drag.current = null;
            }}
        />
    );
}
