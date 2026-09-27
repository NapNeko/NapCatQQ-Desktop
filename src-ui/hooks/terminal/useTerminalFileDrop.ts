// 往终端面板里拖文件。窗口开着 Tauri 的原生拖放，网页里拿不到 HTML5 drop，
// 这里按松手的位置找落在哪个终端格子上（`data-terminal-drop="<会话 id>"`，文件栏再带
// `data-terminal-drop-dir` 指明目录），交给调用方处理。

import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useEffect, useRef, useState } from 'react';

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

export function useTerminalFileDrop(active: boolean, onDrop: (target: TerminalDropTarget, paths: string[]) => void) {
    const [hover, setHover] = useState<TerminalDropTarget | null>(null);
    const onDropRef = useRef(onDrop);
    onDropRef.current = onDrop;

    useEffect(() => {
        if (!active || !isTauri()) {
            setHover(null);
            return;
        }
        let unlisten: (() => void) | undefined;
        let cancelled = false;
        void getCurrentWebview()
            .onDragDropEvent(async (event) => {
                if (cancelled) return;
                const p = event.payload;
                if (p.type === 'leave') {
                    setHover(null);
                    return;
                }
                const factor = await getCurrentWindow().scaleFactor();
                const pos = p.position.toLogical(factor);
                const target = targetAt(pos.x, pos.y);
                if (p.type === 'drop') {
                    setHover(null);
                    if (target && p.paths.length) onDropRef.current(target, p.paths);
                } else {
                    setHover((prev) =>
                        prev?.sessionId === target?.sessionId && prev?.dir === target?.dir ? prev : target,
                    );
                }
            })
            .then((fn) => {
                if (cancelled) fn();
                else unlisten = fn;
            })
            .catch(() => undefined);
        return () => {
            cancelled = true;
            unlisten?.();
            setHover(null);
        };
    }, [active]);

    return hover;
}
