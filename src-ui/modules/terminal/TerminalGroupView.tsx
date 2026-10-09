// 分屏尺寸只在拖动结束时写回会话布局。
import { useLayoutEffect, useRef } from 'react';
import { usePointerDrag } from '../../hooks/ui/usePointerDrag';
import { terminalStore, type TerminalGroup } from '../../hooks/terminal/terminalStore';
import type { TerminalDropTarget } from '../../hooks/terminal/useTerminalFileDrop';
import { cn } from '../../shared/utils/cn';
import { TerminalPane } from './TerminalPane';

export function TerminalGroupView({
    group,
    visible,
    drop,
}: {
    group: TerminalGroup;
    visible: boolean;
    drop: TerminalDropTarget | null;
}) {
    const boxRef = useRef<HTMLDivElement>(null);
    const firstRef = useRef<HTMLDivElement>(null);
    const splitDrag = useRef<{
        rect: DOMRect;
        ratio: number;
        initial: number;
        basis: string;
        pane: HTMLDivElement;
    } | null>(null);
    const divider = usePointerDrag<HTMLDivElement>({
        cursor: group.split === 'row' ? 'col-resize' : 'row-resize',
        onStart: () => {
            const rect = boxRef.current?.getBoundingClientRect();
            const pane = firstRef.current;
            if (!rect || !pane || rect.width === 0 || rect.height === 0) return false;
            splitDrag.current = {
                rect,
                ratio: group.ratio,
                initial: group.ratio,
                basis: pane.style.flexBasis,
                pane,
            };
            return true;
        },
        onMove: (event) => {
            const current = splitDrag.current;
            if (!current) return;
            const { rect } = current;
            const ratio = Math.min(
                0.85,
                Math.max(
                    0.15,
                    group.split === 'row'
                        ? (event.clientX - rect.left) / rect.width
                        : (event.clientY - rect.top) / rect.height,
                ),
            );
            if (ratio === current.ratio) return;
            current.ratio = ratio;
            current.pane.style.flexBasis = `${ratio * 100}%`;
        },
        onEnd: (reason) => {
            const current = splitDrag.current;
            splitDrag.current = null;
            if (!current) return;
            if (reason === 'commit') {
                if (current.ratio !== current.initial)
                    terminalStore.setRatio(group.id, current.ratio);
            } else current.pane.style.flexBasis = current.basis;
        },
    });
    const cancelDivider = divider.cancel;
    useLayoutEffect(() => {
        if (!visible) cancelDivider();
    }, [visible, cancelDivider]);
    useLayoutEffect(() => cancelDivider(), [group.split, cancelDivider]);
    const [first, second] = group.panes;
    const zoneOf = (id: string | undefined) =>
        id && drop?.sessionId === id ? (drop.dir ? 'files' : 'terminal') : null;
    return (
        <div
            ref={boxRef}
            className={cn(
                'ncd-term-group flex min-h-0 min-w-0 flex-1',
                group.split === 'column' ? 'flex-col' : 'flex-row',
            )}
        >
            {first && (
                <div
                    ref={firstRef}
                    className="flex min-h-0 min-w-0"
                    style={
                        second
                            ? { flexBasis: `${group.ratio * 100}%`, flexGrow: 0, flexShrink: 0 }
                            : { flex: 1 }
                    }
                >
                    <TerminalPane
                        sessionId={first}
                        focused={group.focused === first}
                        visible={visible}
                        dropZone={zoneOf(first)}
                        showHeader
                    />
                </div>
            )}
            {second && (
                <>
                    <div
                        className="ncd-term-divider"
                        data-split={group.split}
                        data-dragging={divider.dragging}
                        style={{ touchAction: 'none' }}
                        onPointerDown={divider.start}
                    />
                    <div
                        className="ncd-term-pane-in flex min-h-0 min-w-0 flex-1"
                        data-split={group.split}
                    >
                        <TerminalPane
                            sessionId={second}
                            focused={group.focused === second}
                            visible={visible}
                            dropZone={zoneOf(second)}
                            showHeader
                        />
                    </div>
                </>
            )}
        </div>
    );
}
