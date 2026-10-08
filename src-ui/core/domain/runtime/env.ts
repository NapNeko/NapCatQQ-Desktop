// 运行环境探测：纯 window 检查，零依赖，任何层可 import。

interface TauriInternalsWindow {
    __TAURI_INTERNALS__?: unknown;
}

/// 是否运行在 Tauri webview 内（vs 浏览器预览模式）。
export const isTauri =
    typeof window !== 'undefined' &&
    (window as Window & TauriInternalsWindow).__TAURI_INTERNALS__ !== undefined;
