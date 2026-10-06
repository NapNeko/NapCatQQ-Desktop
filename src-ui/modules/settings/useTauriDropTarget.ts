// Tauri webview 文件拖放：导入向导打开时接收 drop，悬停高亮按逻辑坐标命中拖放区。

import type { RefObject } from 'react';
import { useEffect, useState } from 'react';
import { useFileDragDrop } from '../../hooks/ui/useTauriFileDrop';

export type DroppedKind = 'zip' | 'folder' | 'unknown';

export function classifyDroppedPath(path: string): DroppedKind {
    const lower = path.toLowerCase().replace(/\\/g, '/');
    if (lower.endsWith('.zip')) return 'zip';
    return 'folder';
}

export function pickBestDropPath(paths: string[]): string | null {
    if (!paths.length) return null;
    const zip = paths.find((p) => classifyDroppedPath(p) === 'zip');
    return zip ?? paths[0] ?? null;
}

function pointInRect(rect: DOMRect, x: number, y: number): boolean {
    return x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom;
}

/**
 * `active` 为 true 时订阅 webview 拖放。drop 在向导打开期间直接处理（不依赖命中检测，避免 DPI 坐标偏差）。
 * enter/over 仍用于拖放区高亮（逻辑坐标 + 兜底：拖入窗口即高亮）。
 */
export function useTauriDropTarget(
    active: boolean,
    containerRef: RefObject<HTMLElement | null>,
    onDropPath: (path: string, kind: DroppedKind) => void,
) {
    const [dragHover, setDragHover] = useState(false);

    useEffect(() => {
        if (!active) setDragHover(false);
        return () => setDragHover(false);
    }, [active]);

    useFileDragDrop(active, (event) => {
        if (event.type === 'leave') {
            setDragHover(false);
            return;
        }
        if (event.type === 'enter' || event.type === 'over') {
            const el = containerRef.current;
            // 拿不到拖放区或坐标换算失败时，拖进窗口就算悬停
            if (!el || !event.position) {
                setDragHover(true);
                return;
            }
            setDragHover(
                pointInRect(el.getBoundingClientRect(), event.position.x, event.position.y),
            );
            return;
        }
        setDragHover(false);
        const path = pickBestDropPath(event.paths);
        if (path) onDropPath(path, classifyDroppedPath(path));
    });

    return { dragHover };
}
