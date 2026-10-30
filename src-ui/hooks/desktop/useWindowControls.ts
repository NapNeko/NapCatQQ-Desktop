// 自定义窗口壳：一个 Provider 管理每个原生窗口的状态与操作，标题栏和边缘句柄共用。

import {
    createContext,
    createElement,
    useCallback,
    useContext,
    useEffect,
    useMemo,
    useState,
    type ReactNode,
} from 'react';
import {
    windowControlService,
    type WindowFrameState,
    type WindowResizeDirection,
} from '../../core/services/desktop.service';

export interface WindowControls extends WindowFrameState {
    ready: boolean;
    minimize: () => void;
    toggleMaximize: () => void;
    close: () => void;
    /** 工具窗只关本窗，不走主窗的托盘隐藏 / 退出策略。 */
    closeSelf: () => void;
    startResizeDragging: (direction: WindowResizeDirection) => void;
}

const WindowControlsContext = createContext<WindowControls | null>(null);

/** 启动门在 splash 首帧上屏后调一次；失败只记日志，窗口起不来也没处弹条。 */
export function revealMainWindow(): void {
    void windowControlService.revealMainWindow().catch((err) => {
        console.error('[AppBootGate] 显示主窗口失败:', err);
    });
}

export function WindowControlsProvider({ children }: { children: ReactNode }) {
    const [frame, setFrame] = useState<WindowFrameState | null>(null);

    useEffect(() => {
        let cancelled = false;
        let unlisten: (() => void) | undefined;
        let timer: number | undefined;
        let version = 0;
        const refresh = async () => {
            const current = ++version;
            const next = await windowControlService.frameState();
            if (!cancelled && current === version) setFrame(next);
        };
        const report = (error: unknown) => {
            if (!cancelled) console.warn('窗口状态订阅失败:', error);
        };
        // 拖动尺寸期间只在最后一次事件后查询，避免每一帧重复发窗口状态 IPC。
        void windowControlService
            .onResize(() => {
                if (cancelled) return;
                window.clearTimeout(timer);
                timer = window.setTimeout(() => void refresh().catch(report), 60);
            })
            .then((stop) => {
                if (cancelled) stop();
                else unlisten = stop;
            })
            .catch(report);
        void refresh().catch(report);
        return () => {
            cancelled = true;
            window.clearTimeout(timer);
            unlisten?.();
        };
    }, []);

    const minimize = useCallback(() => void windowControlService.minimize(), []);
    const toggleMaximize = useCallback(() => {
        void windowControlService.toggleMaximize().then((latest) => {
            if (latest !== null) {
                setFrame((previous) => previous && { ...previous, isMaximized: latest });
            }
        });
    }, []);
    const close = useCallback(() => void windowControlService.close(), []);
    const closeSelf = useCallback(() => void windowControlService.closeSelf(), []);
    const startResizeDragging = useCallback((direction: WindowResizeDirection) => {
        void windowControlService.startResizeDragging(direction).catch((error) => {
            console.error('调整窗口尺寸失败:', error);
        });
    }, []);
    const value = useMemo<WindowControls>(
        () => ({
            ready: frame !== null,
            isMaximized: frame?.isMaximized ?? false,
            isFullscreen: frame?.isFullscreen ?? false,
            isResizable: frame?.isResizable ?? false,
            minimize,
            toggleMaximize,
            close,
            closeSelf,
            startResizeDragging,
        }),
        [frame, minimize, toggleMaximize, close, closeSelf, startResizeDragging],
    );
    return createElement(WindowControlsContext.Provider, { value }, children);
}

export function useWindowControls(): WindowControls {
    const controls = useContext(WindowControlsContext);
    if (!controls) throw new Error('窗口控件需要 WindowControlsProvider');
    return controls;
}
