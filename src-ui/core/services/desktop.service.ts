// 桌面壳：窗口控制 + 托盘行为（IPC 字符串集中在此 service）。

import type { SnowLumaWebuiEndpoint } from '../ipc/generated/SnowLumaWebuiEndpoint';
import { preferencesStore } from '../../hooks/preferences/preferencesStore';
import { invoke, isTauri } from '../ipc/transport';

export interface LogSnapshot {
    lines: string[];
    total_lines: number;
}

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

    /** 标题栏关闭：按偏好走「隐藏到托盘」或「退出程序」。 */
    close: async (): Promise<void> => {
        if (!isTauri) return;
        const action = preferencesStore.get().closeAction;
        try {
            if (action === 'tray') {
                await invoke<void>('window_hide_to_tray');
            } else {
                await invoke<void>('request_exit_app');
            }
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
    hideMainWindow: (): Promise<void> => invoke<void>('window_hide_to_tray'),
    countLocalActiveBots: (): Promise<number> =>
        invoke<number>('count_local_active_bots'),
    requestExit: (): Promise<void> => invoke<void>('request_exit_app'),
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