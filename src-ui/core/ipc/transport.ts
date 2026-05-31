// NapCatQQ Desktop - IPC Transport
//
// 这是 Tauri IPC 的最薄壳：只暴露 invoke / listen / isTauri，不带任何业务字符串。
// 业务命令名只允许出现在 `core/services/*.service.ts`。
//
// 浏览器预览时（非 Tauri webview）由 services 层判断 isTauri，并 fallback 到
// `core/ipc/mock/*` 提供的纯前端假数据。

import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { listen as tauriListen, type UnlistenFn } from '@tauri-apps/api/event';

/// 是否运行在 Tauri webview 内（vs 浏览器预览模式）。
export const isTauri =
    typeof window !== 'undefined' && (window as any).__TAURI_INTERNALS__ !== undefined;

/// 透传到 `@tauri-apps/api/core` 的 invoke。
/// services 层应在 `if (isTauri)` 分支调用本函数；否则走 mock。
export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
    return tauriInvoke<T>(cmd, args);
}

/// Tauri event listener 薄壳。services / hooks 都不该直接 import
/// `@tauri-apps/api/event`，统一走这里以便集中替换/打桩。
export async function listen<T = unknown>(
    event: string,
    handler: (payload: T) => void,
): Promise<UnlistenFn> {
    return tauriListen<string>(event, (raw) => {
        // Tauri v2 + serde_json::to_string 序列化的 payload 是字符串，需要手动 parse。
        try {
            const text = raw.payload;
            const parsed = typeof text === 'string' ? (JSON.parse(text) as T) : (text as T);
            handler(parsed);
        } catch (err) {
            // eslint-disable-next-line no-console
            console.error(`[ipc/transport] failed to parse payload of event ${event}:`, err, raw);
        }
    });
}

/// 打开外部 URL（系统默认浏览器）。Tauri webview 不支持 `<a target="_blank">`，
/// 必须走 `tauri-plugin-opener`，capabilities 已在 src-tauri 配好。
export async function openExternalUrl(url: string): Promise<void> {
    const { openUrl } = await import('@tauri-apps/plugin-opener');
    return openUrl(url);
}

/// 弹原生目录选择对话框，返回所选绝对路径；用户取消返回 null。
/// 走 tauri-plugin-dialog 的 `open` 命令（directory 模式）。webview 无法用
/// `<input type=file>` 拿真实文件系统路径，必须走插件。capabilities 已配
/// `dialog:allow-open`。命令名字面量集中在 transport，不外泄到 services。
export async function pickDirectory(title: string): Promise<string | null> {
    const selected = await tauriInvoke<string | string[] | null>('plugin:dialog|open', {
        options: { directory: true, multiple: false, title },
    });
    if (Array.isArray(selected)) return selected[0] ?? null;
    return selected;
}
