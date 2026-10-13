import { useCallback, useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { botService } from '../../core/services/bot.service';
import { errorText } from '../../core/domain/errors';
import { briefError } from '../../core/domain/ui/errorBarCopy';
import { pushInfoBar } from '../ui/globalInfoBarStore';
import { pushErrorBar } from '../ui/pushErrorBar';
import { napcatLoginStore } from '../webui/napcatLoginStore';
import { snowlumaStore } from '../webui/snowlumaStore';
import { botSnapshotsKey } from './useBotSnapshots';

export function useRefreshBotStatuses() {
    const queryClient = useQueryClient();
    const inFlight = useRef(false);
    const [refreshing, setRefreshing] = useState(false);
    const refresh = useCallback(async () => {
        if (inFlight.current) return;
        inFlight.current = true;
        setRefreshing(true);
        try {
            let result;
            try {
                result = await botService.refreshStatuses();
            } finally {
                // 部分失败仍需回读已完成的探测，以及后端恢复后的端点和登录态。
                await Promise.all([
                    queryClient.invalidateQueries(
                        { queryKey: botSnapshotsKey },
                        { throwOnError: true },
                    ),
                    napcatLoginStore.refresh(),
                    snowlumaStore.refresh(),
                ]);
            }
            if (result.failed.length > 0) {
                pushErrorBar({
                    key: 'bot-status-refresh',
                    title: `状态刷新：${result.succeeded.length} 个成功，${result.failed.length} 个失败`,
                    raw: result.failed.map(([id, reason]) => `${id}：${reason}`).join('\n'),
                    content: result.failed
                        .map(([id, reason]) => `${id}：${briefError(reason)}`)
                        .join('；'),
                });
            } else {
                pushInfoBar({
                    key: 'bot-status-refresh',
                    tone: 'success',
                    title: 'Bot 状态已刷新',
                    content: `已检查 ${result.succeeded.length} 个 Bot。`,
                });
            }
        } catch (err) {
            pushErrorBar({ key: 'bot-status-refresh', title: '状态刷新失败', raw: errorText(err) });
        } finally {
            inFlight.current = false;
            setRefreshing(false);
        }
    }, [queryClient]);
    return { refresh, refreshing };
}
