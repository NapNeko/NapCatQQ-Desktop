// 打开外部 URL 的极薄 hook。
//
// features 层不允许直接 import core/ipc/transport（分层铁律），但 hooks 层可以。
// 关于页"查看 release / 在 GitHub 打开"这类外链跳转走这里。webview 里 <a target>
// 不会用系统浏览器开，必须走 Tauri opener 插件（R4）。

import { useCallback } from 'react';
import { openExternalUrl } from '../../core/ipc/transport';

export function useOpenExternal() {
    return useCallback(async (url: string) => {
        try {
            await openExternalUrl(url);
        } catch (err) {
            // eslint-disable-next-line no-console
            console.warn('打开外部链接失败:', url, err);
        }
    }, []);
}
