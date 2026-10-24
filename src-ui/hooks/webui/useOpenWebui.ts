// 打开 Bot WebUI 的统一动作 hook（NapCat / SnowLuma 二合一）。
//
// - NapCat：URL 自带 token 自动登录，直接 openExternalUrl。
// - SnowLuma：调后端解析 url + password；密码复制剪贴板，再 openExternalUrl。

import { useCallback } from 'react';
import { openExternalUrl } from '../../core/ipc/transport';
import { snowlumaService } from '../../core/services/desktop.service';
import { clipboardService } from '../../core/services/clipboard.service';
import { buildNapcatWebuiUrl, type NapcatWebuiBinding } from '../../core/domain/webui/availability';
import type { Flavor } from '../../core/domain/bot/flavor';
import { pushInfoBar } from '../ui/globalInfoBarStore';

export interface OpenWebuiArgs {
    botId: string;
    flavor: Flavor | null | undefined;
    napcat?: NapcatWebuiBinding | null;
}

export function useOpenWebui() {
    return useCallback(async ({ botId, flavor, napcat }: OpenWebuiArgs): Promise<void> => {
        if (flavor === 'snowluma') {
            const ep = await snowlumaService.openWebui(botId);
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
                    key: `snowluma-webui-clipboard:${botId}`,
                    tone: copied && opened ? 'success' : 'warning',
                    title: copied
                        ? '密码已复制'
                        : copyState === 'missing'
                          ? '未读取到登录密码'
                          : '未能复制登录密码',
                    content: opened
                        ? copied
                            ? '粘贴到 SnowLuma WebUI 登录。'
                            : 'WebUI 已打开，请使用你设置的密码登录。'
                        : copied
                          ? '浏览器未能打开，请重试打开 WebUI。'
                          : '浏览器未能打开，剪贴板未更新。',
                    autoDismissMs: copied && opened ? 3000 : undefined,
                });
            }
            return;
        }

        if (!napcat) {
            throw new Error('NapCat WebUI 链接尚未就绪');
        }
        await openExternalUrl(buildNapcatWebuiUrl(napcat));
    }, []);
}
