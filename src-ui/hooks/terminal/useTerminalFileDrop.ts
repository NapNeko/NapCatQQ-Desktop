// 往终端面板里拖文件。窗口开着 Tauri 的原生拖放，网页里拿不到 HTML5 drop，
// 这里按松手的位置找落在哪个终端格子上（`data-terminal-drop="<会话 id>"`，文件栏再带
// `data-terminal-drop-dir` 指明目录），交给调用方处理。

import { useEffect, useState } from 'react';
import { useFileDragDrop } from '../ui/useTauriFileDrop';

export interface TerminalDropTarget {
    sessionId: string;
    /** 落在文件栏上时是文件栏当前的目录 */
    dir: string | null;
}

function targetAt(x: number, y: number): TerminalDropTarget | null {
    const el = document.elementFromPoint(x, y)?.closest<HTMLElement>('[data-terminal-drop]');
    if (!el?.dataset.terminalDrop) return null;
    return { sessionId: el.dataset.terminalDrop, dir: el.dataset.terminalDropDir || null };
}

export function useTerminalFileDrop(
    active: boolean,
    onDrop: (target: TerminalDropTarget, paths: string[]) => void,
) {
    const [hover, setHover] = useState<TerminalDropTarget | null>(null);

    useEffect(() => {
        if (!active) setHover(null);
        return () => setHover(null);
    }, [active]);

    useFileDragDrop(active, (event) => {
        if (event.type === 'leave') {
            setHover(null);
            return;
        }
        const target = event.position ? targetAt(event.position.x, event.position.y) : null;
        if (event.type === 'drop') {
            setHover(null);
            if (target && event.paths.length) onDrop(target, event.paths);
        } else {
            setHover((prev) =>
                prev?.sessionId === target?.sessionId && prev?.dir === target?.dir ? prev : target,
            );
        }
    });

    return hover;
}
