// 桌面壳侧 IPC 服务（窗口控制 / 诊断事件 / SnowLuma WebUI 端点）。
//
// 这些是"系统级 / 跨业务域"杂项，单独放进来避免其它 service 文件膨胀。
// 把 `@tauri-apps/api/window` 和 `tauri://resize` 字面量集中到这里，
// 保持组件层只通过 service 调用（满足 frontend-layering 铁律）。

import { invoke, isTauri } from '../ipc/transport';

// ─── 窗口控制 ────────────────────────────────────────────────────────────

type WindowController = {
    minimize: () => Promise<void>;
    toggleMaximize: () => Promise<void>;
    close: () => Promise<void>;
    isMaximized: () => Promise<boolean>;
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

    close: async (): Promise<void> => {
        const w = await getWindow();
        if (!w) return;
        try {
            await w.close();
        } catch (err) {
            console.error('窗口关闭失败:', err);
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

    /// 监听 `tauri://resize` 事件并回调最新的最大化状态。
    /// 浏览器预览模式直接返回 noop。
    onResize: async (cb: (isMaximized: boolean) => void): Promise<() => void> => {
        const w = await getWindow();
        if (!w) return () => { };
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
            return () => { };
        }
    },
};

// ─── 诊断 / Demo 事件 ────────────────────────────────────────────────────

export const diagnosticsService = {
    publishRuntimeStatus: async (): Promise<void> => {
        if (isTauri) return invoke<void>('publish_runtime_status');
    },

    publishDemoEvent: async (): Promise<void> => {
        if (isTauri) return invoke<void>('publish_demo_event');
    },
};

// ─── SnowLuma WebUI 端点 ────────────────────────────────────────────────

export interface SnowLumaWebuiEndpoint {
    url: string;
    password: string;
}

export const snowlumaService = {
    /// 解析 SnowLuma WebUI 的 url 和登录密码。
    /// UI 拿到后应：1) 写密码到剪贴板；2) `openExternalUrl(url)`。
    openWebui: async (botId: string): Promise<SnowLumaWebuiEndpoint> => {
        if (isTauri) {
            return invoke<SnowLumaWebuiEndpoint>('open_snowluma_webui', { botId });
        }
        return Promise.resolve({
            url: `http://127.0.0.1:6099/webui?mock=${botId}`,
            password: 'mock-password-snowluma',
        });
    },
};
