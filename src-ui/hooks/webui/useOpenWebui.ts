// 打开 Bot WebUI 的统一动作 hook（NapCat / SnowLuma 二合一）。
//
// - NapCat：URL 自带 token 自动登录，直接 openExternalUrl。
// - SnowLuma：调后端解析 url + password；密码复制剪贴板，再 openExternalUrl。

import { useCallback } from 'react';
import { openExternalUrl } from '../../core/ipc/transport';
import { snowlumaService } from '../../core/services/desktop.service';
import {
    buildNapcatWebuiUrl,
    type NapcatWebuiBinding,
} from '../../core/domain/webui/availability';
import type { Flavor } from '../../core/domain/bot/flavor';

export interface OpenWebuiArgs {
    botId: string;
    flavor: Flavor | null | undefined;
    napcat?: NapcatWebuiBinding | null;
}

export function useOpenWebui() {
    return useCallback(async ({ botId, flavor, napcat }: OpenWebuiArgs): Promise<void> => {
        if (flavor === 'snowluma') {
            try {
                const ep = await snowlumaService.openWebui(botId);
                try {
                    await navigator.clipboard.writeText(ep.password);
                } catch (e) {
                    // eslint-disable-next-line no-console
                    console.warn('密码写入剪贴板失败:', e);
                }
                await openExternalUrl(ep.url);
            } catch (err) {
                // eslint-disable-next-line no-console
                console.error('打开 SnowLuma WebUI 失败:', err);
                // 让上层决定怎么提示，这里只 throw。
                throw err;
            }
            return;
        }

        if (!napcat) {
            throw new Error('NapCat WebUI 链接尚未就绪');
        }
        await openExternalUrl(buildNapcatWebuiUrl(napcat));
    }, []);
}
