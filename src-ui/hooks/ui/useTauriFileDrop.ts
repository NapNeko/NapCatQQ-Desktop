// 往窗口里拖文件：拖进来时亮一层提示，松手把所有路径交给调用方。Tauri 的原生拖放拿的是本机路径，
// webview 里的 HTML5 drop 拿不到（窗口开着 dragDropEnabled）。浏览器预览里什么都不做。

import { useEffect, useRef, useState } from 'react';
import { isTauri, onFileDragDrop, type FileDragDropEvent } from '../../core/ipc/transport';

export type { FileDragDropEvent };

/** `active` 时订阅原生拖放，关掉或卸载时退订；各种拖放区（终端格子、导入向导）都从这里接 */
export function useFileDragDrop(active: boolean, onEvent: (event: FileDragDropEvent) => void): void {
    const onEventRef = useRef(onEvent);
    onEventRef.current = onEvent;

    useEffect(() => {
        if (!active || !isTauri) return;
        let unlisten: (() => void) | undefined;
        let cancelled = false;
        void onFileDragDrop((event) => {
            if (!cancelled) onEventRef.current(event);
        })
            .then((fn) => {
                // 订阅还没回来就关掉了，回来时立刻退掉，别留一个没人收的监听
                if (cancelled) fn();
                else unlisten = fn;
            })
            .catch((err) => {
                // eslint-disable-next-line no-console
                console.warn('[useFileDragDrop] onDragDropEvent failed:', err);
            });
        return () => {
            cancelled = true;
            unlisten?.();
        };
    }, [active]);
}

export function useTauriFileDrop(active: boolean, onDrop: (paths: string[]) => void) {
    const [dragging, setDragging] = useState(false);

    useEffect(() => {
        if (!active) setDragging(false);
        return () => setDragging(false);
    }, [active]);

    useFileDragDrop(active, (event) => {
        if (event.type === 'enter' || event.type === 'over') setDragging(true);
        else if (event.type === 'leave') setDragging(false);
        else {
            setDragging(false);
            if (event.paths.length > 0) onDrop(event.paths);
        }
    });

    return { dragging };
}
