// 自定义标题栏：窗口控制 + 关闭行为（托盘 / 退出）。

import { useCallback, useEffect, useState } from 'react';
import { windowControlService } from '../../core/services/desktop.service';

export interface WindowControls {
    isMaximized: boolean;
    minimize: () => void;
    toggleMaximize: () => void;
    close: () => void;
}

/** 启动门在 splash 首帧上屏后调一次；失败只记日志，窗口起不来也没处弹条 */
export function revealMainWindow(): void {
    void windowControlService.revealMainWindow().catch((err) => {
        console.error('[AppBootGate] 显示主窗口失败:', err);
    });
}

export function useWindowControls(): WindowControls {
    const [isMaximized, setIsMaximized] = useState(false);

    useEffect(() => {
        let cancelled = false;
        let unlisten: (() => void) | undefined;

        const setup = async () => {
            const initial = await windowControlService.isMaximized();
            if (cancelled) return;
            setIsMaximized(initial);
            const stop = await windowControlService.onResize((latest) => {
                if (!cancelled) setIsMaximized(latest);
            });
            if (cancelled) stop();
            else unlisten = stop;
        };

        void setup().catch((error) => {
            if (!cancelled) console.warn('窗口控制订阅失败:', error);
        });
        return () => {
            cancelled = true;
            unlisten?.();
        };
    }, []);

    const minimize = useCallback(() => {
        void windowControlService.minimize();
    }, []);

    const toggleMaximize = useCallback(() => {
        void windowControlService.toggleMaximize().then((latest) => {
            if (latest !== null) setIsMaximized(latest);
        });
    }, []);

    const close = useCallback(() => {
        void windowControlService.close();
    }, []);

    return { isMaximized, minimize, toggleMaximize, close };
}
