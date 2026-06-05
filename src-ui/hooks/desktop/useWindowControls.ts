// 自定义标题栏：窗口控制 + 关闭行为（托盘 / 退出）。

import { useCallback, useEffect, useState } from 'react';
import { windowControlService } from '../../core/services/desktop.service';

export interface WindowControls {
    isMaximized: boolean;
    minimize: () => void;
    toggleMaximize: () => void;
    close: () => void;
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
            unlisten = await windowControlService.onResize((latest) => {
                if (!cancelled) setIsMaximized(latest);
            });
        };

        void setup();
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