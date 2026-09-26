// 往窗口里拖文件：拖进来时亮一层提示，松手把所有路径交给调用方。Tauri 的原生拖放拿的是本机路径，
// webview 里的 HTML5 drop 拿不到（窗口开着 dragDropEnabled）。浏览器预览里什么都不做。

import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { useEffect, useRef, useState } from 'react';

export function useTauriFileDrop(active: boolean, onDrop: (paths: string[]) => void) {
    const [dragging, setDragging] = useState(false);
    const onDropRef = useRef(onDrop);
    onDropRef.current = onDrop;

    useEffect(() => {
        if (!active || !isTauri()) {
            setDragging(false);
            return;
        }
        let unlisten: (() => void) | undefined;
        let cancelled = false;
        void getCurrentWebview()
            .onDragDropEvent((event) => {
                if (cancelled) return;
                const p = event.payload;
                if (p.type === 'enter' || p.type === 'over') setDragging(true);
                else if (p.type === 'leave') setDragging(false);
                else if (p.type === 'drop') {
                    setDragging(false);
                    if (p.paths.length > 0) onDropRef.current(p.paths);
                }
            })
            .then((fn) => {
                if (cancelled) fn();
                else unlisten = fn;
            })
            .catch((err) => {
                // eslint-disable-next-line no-console
                console.warn('[useTauriFileDrop] onDragDropEvent failed:', err);
            });
        return () => {
            cancelled = true;
            unlisten?.();
            setDragging(false);
        };
    }, [active]);

    return { dragging };
}
