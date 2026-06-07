// 打开外部链接的统一 hook。
//
// Tauri webview 不支持 `<a target="_blank">`(点了没反应),外链必须走系统 opener。
// modules 层不直接 import core/ipc,统一通过这层调用;openExternalUrl 内含
// http/https scheme 白名单,被拒(非法 / 危险 scheme)时弹红条提示而不是静默失败。

import { useCallback } from 'react';
import { openExternalUrl } from '../core/ipc/transport';
import { pushInfoBar } from './ui/globalInfoBarStore';

export function useOpenExternal() {
    return useCallback((url: string) => {
        void openExternalUrl(url).catch((err) => {
            pushInfoBar({
                tone: 'danger',
                title: '无法打开链接',
                content: err instanceof Error ? err.message : String(err),
            });
        });
    }, []);
}
