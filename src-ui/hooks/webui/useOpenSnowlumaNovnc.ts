// 打开 SnowLuma Docker noVNC 扫码页（URL 已带 VNC 密码 query，对齐 legacy vnc_launcher）。

import { useCallback } from 'react';
import { openExternalUrl } from '../../core/ipc/transport';
import { snowlumaService } from '../../core/services/desktop.service';
import { clipboardService } from '../../core/services/clipboard.service';
import { pushInfoBar } from '../ui/globalInfoBarStore';

export function useOpenSnowlumaNovnc() {
    return useCallback(async (botId: string): Promise<void> => {
        const ep = await snowlumaService.openNovnc(botId);
        let copyState: 'copied' | 'missing' | 'failed' = 'missing';
        if (ep.password.trim()) {
            try {
                await clipboardService.writeText(ep.password);
                copyState = 'copied';
            } catch {
                copyState = 'failed';
            }
        }
        let opened = false;
        try {
            await openExternalUrl(ep.url);
            opened = true;
        } finally {
            const copied = copyState === 'copied';
            pushInfoBar({
                key: `snowluma-novnc-clipboard:${botId}`,
                tone: copied && opened ? 'success' : 'warning',
                title: copied
                    ? 'VNC 密码已复制'
                    : copyState === 'missing'
                      ? '未读取到 VNC 密码'
                      : '未能复制 VNC 密码',
                content: opened
                    ? copied
                        ? '需要验证时粘贴密码。'
                        : 'QQ 界面已打开，密码未复制。'
                    : copied
                      ? '浏览器未能打开，请重试打开 QQ 界面。'
                      : '浏览器未能打开，剪贴板未更新。',
                autoDismissMs: copied && opened ? 3000 : undefined,
            });
        }
    }, []);
}
