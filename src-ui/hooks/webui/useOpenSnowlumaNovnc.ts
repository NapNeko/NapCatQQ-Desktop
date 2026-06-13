// 打开 SnowLuma Docker noVNC 扫码页（URL 已带 VNC 密码 query，对齐 legacy vnc_launcher）。

import { useCallback } from 'react';
import { openExternalUrl } from '../../core/ipc/transport';
import { snowlumaService } from '../../core/services/desktop.service';

export function useOpenSnowlumaNovnc() {
    return useCallback(async (botId: string): Promise<void> => {
        const ep = await snowlumaService.openNovnc(botId);
        try {
            await navigator.clipboard.writeText(ep.password);
        } catch (e) {
            // eslint-disable-next-line no-console
            console.warn('VNC 密码写入剪贴板失败:', e);
        }
        await openExternalUrl(ep.url);
    }, []);
}