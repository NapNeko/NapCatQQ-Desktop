// 托盘面板窗口的动作：调窗高、回主窗、进轻量、退出，以及面板里直接启停 Bot。
// 面板是单独的小窗口，出错由面板自己在卡片里提示，这里只把失败抛回去。

import { useEffect, useRef } from 'react';
import { useMutation } from '@tanstack/react-query';
import { botService } from '../../core/services/bot.service';
import { trayService, windowEventService } from '../../core/services/desktop.service';

let lastReportedHeight = 0;

/** 按卡片的自然高度调窗；抖动不到 2px 不报，浏览器预览里没有 IPC 就算了 */
export function reportTrayPanelHeight(el: HTMLElement): void {
    const h = Math.ceil(el.getBoundingClientRect().height);
    if (h <= 60 || h >= 600 || Math.abs(h - lastReportedHeight) < 2) return;
    lastReportedHeight = h;
    void trayService.resizePanel(h).catch(() => undefined);
}

/** 面板每次展开时后端发一条，拿来刷新列表、校准高度 */
export function useTrayPanelShown(onShown: () => void): void {
    const onShownRef = useRef(onShown);
    onShownRef.current = onShown;

    useEffect(() => {
        let unlisten: (() => void) | undefined;
        let cancelled = false;
        void windowEventService
            .onTrayPanelShow(() => onShownRef.current())
            .then((fn) => {
                if (cancelled) fn();
                else unlisten = fn;
            });
        return () => {
            cancelled = true;
            unlisten?.();
        };
    }, []);
}

export function useTrayPanelActions() {
    const start = useMutation({ mutationFn: botService.start });
    const stop = useMutation({ mutationFn: botService.stop });

    return {
        startBot: start.mutateAsync,
        stopBot: stop.mutateAsync,
        // 这三个点了面板就收起或进程要走，失败也没处提示
        showMainWindow: () => trayService.showMainWindow().catch(() => undefined),
        enterLightweight: () => trayService.enterLightweight().catch(() => undefined),
        quit: () => trayService.quit().catch(() => undefined),
    };
}
