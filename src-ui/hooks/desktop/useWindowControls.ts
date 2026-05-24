// 自定义标题栏的 React 适配。把 windowControlService 的命令式 API 包成 React-friendly：
//   - 三个 action 直接暴露
//   - isMaximized 作为 reactive state 返回，subscribe 在 mount 时挂上
//
// 严守 frontend-layering 边界：组件不再 import `@tauri-apps/api/*`，只通过本 hook。

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
