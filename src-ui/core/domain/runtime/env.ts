// 运行环境探测：纯 window 检查，零依赖，任何层可 import。

interface TauriInternalsWindow {
    __TAURI_INTERNALS__?: unknown;
}

/// 是否运行在 Tauri webview 内（vs 浏览器预览模式）。
export const isTauri =
    typeof window !== 'undefined' &&
    (window as Window & TauriInternalsWindow).__TAURI_INTERNALS__ !== undefined;

/** WebView 所在桌面的系统；远端能力以主机探测结果为准。 */
export function localOs(): 'windows' | 'linux' | 'mac_os' {
    const ua = typeof navigator === 'undefined' ? '' : navigator.userAgent.toLowerCase();
    if (ua.includes('linux')) return 'linux';
    if (ua.includes('mac os') || ua.includes('macintosh')) return 'mac_os';
    return 'windows';
}
