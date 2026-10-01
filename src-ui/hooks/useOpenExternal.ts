// 打开外部链接的统一 hook。
//
// Tauri webview 不支持 `<a target="_blank">`(点了没反应),外链必须走系统 opener。
// modules 层不直接 import core/ipc,统一通过这层调用;openExternalUrl 内含
// http/https scheme 白名单,被拒(非法 / 危险 scheme)时弹红条提示而不是静默失败。

import { useCallback } from 'react';
import { openExternalUrl, openWindowsAppsSettings } from '../core/ipc/transport';
import { pushErrorBar } from './ui/pushErrorBar';
import { errorText } from '../core/domain/errors';

// 不在组件里的调用方（终端、Docker 下载页）也走这一个口子；带 key 是为了连点几次只留一条
export function openExternalOrReport(url: string): void {
    void openExternalUrl(url).catch((err) => {
        pushErrorBar({
            key: 'open-external-failed',
            title: '无法打开链接',
            raw: errorText(err),
        });
    });
}

/// 打开系统「已安装的应用」页（系统管理组件的卸载入口），失败弹错与外链同口径
export function openSystemUninstallOrReport(): void {
    void openWindowsAppsSettings().catch((err) => {
        pushErrorBar({
            key: 'open-system-uninstall-failed',
            title: '无法打开系统卸载页',
            raw: errorText(err),
        });
    });
}

export function useOpenExternal() {
    return useCallback((url: string) => openExternalOrReport(url), []);
}
