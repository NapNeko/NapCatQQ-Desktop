// 桌面壳：窗口控制 + 托盘行为（IPC 字符串集中在此 service）。

import type { DesktopExitBlocked } from '../ipc/generated/DesktopExitBlocked';
import type { SnowLumaWebuiEndpoint } from '../ipc/generated/SnowLumaWebuiEndpoint';
import type { WindowSignal } from '../ipc/generated/WindowSignal';
import type { LogSnapshot } from '../ipc/types';
import { preferencesStore } from '../../hooks/preferences/preferencesStore';
import { invoke, isTauri, listen } from '../ipc/transport';

// 不走 DomainEvent 总线的几条窗口通知，名字和信封版本对应 src-tauri/src/window_events.rs，
// 那边的测试会读这个文件核对，改名时两边一起改
const WINDOW_EVENT = {
    requestClose: 'desktop-request-close',
    exitBlocked: 'desktop-exit-blocked',
    trayPanelShow: 'tray_panel_show',
    debugPopoutClosed: 'debug-popout-closed',
} as const;

const WINDOW_SIGNAL: WindowSignal = { v: 1 };

type WindowController = {
    minimize: () => Promise<void>;
    toggleMaximize: () => Promise<void>;
    close: () => Promise<void>;
    isMaximized: () => Promise<boolean>;
    hide: () => Promise<void>;
    show: () => Promise<void>;
};

async function getWindow(): Promise<WindowController | null> {
    try {
        const { getCurrentWindow } = await import('@tauri-apps/api/window');
        return getCurrentWindow() as WindowController;
    } catch {
        return null;
    }
}

export const windowControlService = {
    /** 主窗口建出来是隐藏的，首屏画好后由前端叫它出来；浏览器预览里什么都不做 */
    revealMainWindow: async (): Promise<void> => {
        if (!isTauri) return;
        await invoke<void>('show_main_window');
    },

    minimize: async (): Promise<void> => {
        const w = await getWindow();
        if (!w) return;
        try {
            await w.minimize();
        } catch (err) {
            console.error('窗口最小化失败:', err);
        }
    },

    toggleMaximize: async (): Promise<boolean | null> => {
        const w = await getWindow();
        if (!w) return null;
        try {
            await w.toggleMaximize();
            return await w.isMaximized();
        } catch (err) {
            console.error('窗口最大化/还原失败:', err);
            return null;
        }
    },

    /** 标题栏关闭：按偏好走「隐藏到托盘」或「退出程序」（经退出闸门对话框）。 */
    close: async (): Promise<void> => {
        if (!isTauri) return;
        const action = preferencesStore.get().closeAction;
        try {
            if (action === 'tray') {
                await invoke<void>('window_hide_to_tray');
                return;
            }
            const { emit } = await import('@tauri-apps/api/event');
            // 后端关窗时发的是同一个事件，信封也对齐
            await emit(WINDOW_EVENT.requestClose, WINDOW_SIGNAL);
        } catch (err) {
            console.error('关闭窗口失败:', err);
        }
    },

    /** 独立工具窗（调试台弹出窗）标题栏的关闭：只关自己，不走主窗的托盘隐藏 / 退出闸门 */
    closeSelf: async (): Promise<void> => {
        const w = await getWindow();
        if (!w) return;
        try {
            await w.close();
        } catch (err) {
            console.error('关闭窗口失败:', err);
        }
    },

    isMaximized: async (): Promise<boolean> => {
        const w = await getWindow();
        if (!w) return false;
        try {
            return await w.isMaximized();
        } catch {
            return false;
        }
    },

    onResize: async (cb: (isMaximized: boolean) => void): Promise<() => void> => {
        const w = await getWindow();
        if (!w) return () => {};
        try {
            const { listen } = await import('@tauri-apps/api/event');
            const unlisten = await listen('tauri://resize', async () => {
                try {
                    cb(await w.isMaximized());
                } catch (err) {
                    console.error('刷新窗口最大化状态失败:', err);
                }
            });
            return unlisten;
        } catch (err) {
            console.error('初始化标题栏窗口状态失败:', err);
            return () => {};
        }
    },
};

export const trayService = {
    showMainWindow: (): Promise<void> => invoke<void>('window_show'),
    /** 托盘面板按内容高度调窗 */
    resizePanel: (height: number): Promise<void> => invoke<void>('tray_panel_resize', { height }),
    enterLightweight: (): Promise<void> => invoke<void>('tray_panel_enter_lightweight'),
    quit: (): Promise<void> => invoke<void>('tray_panel_quit'),
};

export const windowEventService = {
    /** 关窗动作是「退出」：后端关窗和标题栏关闭都发这条，主窗据此走退出闸门。 */
    onRequestClose: (cb: (signal: WindowSignal) => void): Promise<() => void> =>
        listen<WindowSignal>(WINDOW_EVENT.requestClose, cb),

    /** 托盘退出被本机 Bot 拦下。 */
    onExitBlocked: (cb: (payload: DesktopExitBlocked) => void): Promise<() => void> =>
        listen<DesktopExitBlocked>(WINDOW_EVENT.exitBlocked, cb),

    /** 托盘面板每次展开前后端发给面板窗口；拿不到窗口 API 时返回空退订。 */
    onTrayPanelShow: async (cb: (signal: WindowSignal) => void): Promise<() => void> => {
        try {
            const { getCurrentWindow } = await import('@tauri-apps/api/window');
            const win = getCurrentWindow();
            return await win.listen<WindowSignal>(WINDOW_EVENT.trayPanelShow, (e) => cb(e.payload));
        } catch {
            return () => {};
        }
    },

    /** 调试台弹出窗被销毁后端发给主窗：据此作废调试台的内存状态。弹不出窗口的环境返回空退订。 */
    onDebugPopoutClosed: async (cb: (signal: WindowSignal) => void): Promise<() => void> => {
        try {
            const { getCurrentWindow } = await import('@tauri-apps/api/window');
            const win = getCurrentWindow();
            return await win.listen<WindowSignal>(WINDOW_EVENT.debugPopoutClosed, (e) =>
                cb(e.payload),
            );
        } catch {
            return () => {};
        }
    },
};

export const diagnosticsService = {
    publishDemoEvent: (): Promise<void> => invoke<void>('publish_demo_event'),
    publishRuntimeStatus: (): Promise<void> => invoke<void>('publish_runtime_status'),
};

export const desktopLogService = {
    tailLog: async (
        lines?: number,
        levelFilter?: { level?: string },
    ): Promise<LogSnapshot> => {
        if (isTauri) {
            return invoke<LogSnapshot>('tail_desktop_log', {
                lines: lines ?? 2000,
                levelFilter: levelFilter ?? null,
            });
        }
        return {
            lines: [
                '26-06-10 12:00:00 | [INFO] | [ NONE_TYPE ] | [ CORE ] | [desktop > tracing] | 浏览器预览：Desktop 日志需在 Tauri 中查看\n',
            ],
            total_lines: 1,
        };
    },

    openLogLocation: async (): Promise<string> => {
        if (isTauri) return invoke<string>('open_desktop_log_location');
        return '';
    },
};

export const snowlumaService = {
    openWebui: (botId: string): Promise<SnowLumaWebuiEndpoint> =>
        invoke<SnowLumaWebuiEndpoint>('open_snowluma_webui', { botId }),
    openNovnc: (botId: string): Promise<SnowLumaWebuiEndpoint> =>
        invoke<SnowLumaWebuiEndpoint>('open_snowluma_novnc', { botId }),
};