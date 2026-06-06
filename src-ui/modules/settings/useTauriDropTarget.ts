// Tauri webview 文件拖放：导入向导打开时接收 drop，悬停高亮按逻辑坐标命中拖放区。

import { isTauri } from '@tauri-apps/api/core';
import { PhysicalPosition } from '@tauri-apps/api/dpi';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { getCurrentWindow } from '@tauri-apps/api/window';
import type { RefObject } from 'react';
import { useEffect, useRef, useState } from 'react';

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

async function toLogicalClient(
    position: PhysicalPosition,
): Promise<{ x: number; y: number }> {
    const factor = await getCurrentWindow().scaleFactor();
    const logical = position.toLogical(factor);
    return { x: logical.x, y: logical.y };
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
    const onDropRef = useRef(onDropPath);
    onDropRef.current = onDropPath;

    useEffect(() => {
        if (!active || !isTauri) {
            setDragHover(false);
            return;
        }

        let unlisten: (() => void) | undefined;
        let cancelled = false;

        void getCurrentWebview()
            .onDragDropEvent(async (event) => {
                if (cancelled) return;
                const payload = event.payload;

                if (payload.type === 'leave') {
                    setDragHover(false);
                    return;
                }

                if (payload.type === 'enter' || payload.type === 'over') {
                    const el = containerRef.current;
                    if (!el) {
                        setDragHover(true);
                        return;
                    }
                    try {
                        const { x, y } = await toLogicalClient(payload.position);
                        const rect = el.getBoundingClientRect();
                        const inside = pointInRect(rect, x, y);
                        setDragHover(inside);
                    } catch {
                        setDragHover(true);
                    }
                    return;
                }

                if (payload.type === 'drop') {
                    setDragHover(false);
                    const path = pickBestDropPath(payload.paths);
                    if (path) {
                        onDropRef.current(path, classifyDroppedPath(path));
                    }
                }
            })
            .then((fn) => {
                if (!cancelled) unlisten = fn;
            })
            .catch((err) => {
                // eslint-disable-next-line no-console
                console.warn('[useTauriDropTarget] onDragDropEvent failed:', err);
            });

        return () => {
            cancelled = true;
            unlisten?.();
            setDragHover(false);
        };
    }, [active, containerRef]);

    return { dragHover };
}